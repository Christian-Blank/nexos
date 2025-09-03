use clap::{Arg, ArgAction, ArgMatches, Command};
use clap::parser::ValueSource;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as Proc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime, Duration};
use tracing::{span, Level};

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

mod checks;

// ---------- Existing structs for dev init ----------
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DevInitArgs {
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    yes: bool,
    #[serde(default)]
    #[schemars(default)]
    with_config: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DevInitResult {
    planned_actions: Vec<String>,
    applied: bool,
}

// ---------- NEW: plugin scaffolder ----------
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PluginNewArgs {
    name: String,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    register: bool,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    yes: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PluginNewResult {
    planned_actions: Vec<String>,
    created_paths: Vec<String>,
    registered: bool,
}

// ---------- NEW: structure checker ----------
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PluginsCheckArgs {
    #[serde(default)]
    fix_readmes: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PluginCheck {
    plugin: String,
    has_readme: bool,
    has_cargo_toml: bool,
    has_src_lib: bool,
    crate_name_ok: bool,
    bin_dep_ok: bool,
    main_registered: bool,
    errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PluginsCheckResult {
    summary: String,
    total: usize,
    ok: usize,
    not_ok: usize,
    items: Vec<PluginCheck>,
}

// ---------- NEW: doctor command ----------
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorArgs {
    #[serde(default)]
    ci: bool,
    #[serde(default)]
    checks: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorCheck {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorResult {
    ready: bool,
    checks: Vec<DoctorCheck>,
}

// ---------- NEW: Doctor CI structs ----------
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorCiCheck {
    name: String,
    ok: bool,
    detail: String,
    #[serde(default)]
    severity: String,          // "info" | "error"
    #[serde(default)]
    fix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorCiResult {
    ok: bool,
    checks: Vec<DoctorCiCheck>,
}

// ---------- NEW: Telemetry structs ----------
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryReportArgs {
    #[serde(default)]
    since: Option<String>, // "24h" | "7d" | RFC3339
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryByCommand {
    plugin: String,
    command: String,
    count: u64,
    success: u64,
    fail: u64,
    p50_ms: Option<u64>,
    p90_ms: Option<u64>,
    avg_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ErrorCodeRow {
    error_code: String,
    count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SlowRow {
    plugin: String,
    command: String,
    p90_ms: u64,
    count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryReport {
    total_events: u64,
    window_start: Option<String>,
    by_command: Vec<TelemetryByCommand>,
    errors_by_code: Vec<ErrorCodeRow>,
    slowest_commands: Vec<SlowRow>,
}

// ---------- Dev plugin ----------
pub struct DevPlugin;
impl DevPlugin { pub fn new() -> Self { Self } }

impl Plugin for DevPlugin {
    fn name(&self) -> &'static str { "dev" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Developer ergonomics (init, plugin scaffolding, structure checks)" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("dev").about("Developer utilities")
                // dev init
                .subcommand(
                    Command::new("init")
                        .about("Initialize .nexoscli/ (Plan/Apply, idempotent)")
                        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue).help("Show plan only"))
                        .arg(Arg::new("yes").long("yes").action(ArgAction::SetTrue).help("Apply without prompt"))
                        .arg(Arg::new("with-config").long("with-config").action(ArgAction::SetTrue).help("Create .nexoscli/config.toml with defaults"))
                )
                // dev plugin new
                .subcommand(
                    Command::new("plugin")
                        .about("Plugin utilities")
                        .subcommand(
                            Command::new("new")
                                .about("Scaffold a new plugin under plugins/<name>")
                                .arg(Arg::new("name").long("name").required(true).value_name("NAME"))
                                .arg(Arg::new("desc").long("desc").required(false).value_name("TEXT"))
                                .arg(Arg::new("register").long("register").action(ArgAction::SetTrue).help("Attempt to register in workspace, bin Cargo.toml, and main.rs"))
                                .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                                .arg(Arg::new("yes").long("yes").action(ArgAction::SetTrue))
                        )
                )
                // dev plugins check
                .subcommand(
                    Command::new("plugins")
                        .about("Plugins repo checks")
                        .subcommand(
                            Command::new("check")
                                .about("Verify structure and registration for all plugins/*")
                                .arg(Arg::new("fix-readmes").long("fix-readmes").action(ArgAction::SetTrue).help("Create missing README.md with template"))
                        )
                )
                // dev doctor
                .subcommand(
                    Command::new("doctor")
                        .about("Environment & hygiene checks")
                        .arg(Arg::new("since").long("since").default_value("24h"))
                        .arg(Arg::new("require").long("require").help("CSV of checks to enforce").default_value(""))
                        .arg(Arg::new("task_coverage_min").long("task-coverage-min").default_value("0.7"))
                        .arg(Arg::new("output").long("output").value_parser(["text","json"]).default_value("json"))
                        .arg(Arg::new("ci").long("ci").action(ArgAction::SetTrue)
                             .help("Aggregate repo checks and exit non-zero on failure"))
                        .arg(Arg::new("checks").long("checks").value_name("CSV")
                             .help("Subset of checks: graph,plugins,config,events,tools,cline (default: all)"))
                )
                // dev telemetry
                .subcommand(
                    Command::new("telemetry").about("Observability and analytics")
                        .subcommand(
                            Command::new("report")
                                .about("Aggregate events.jsonl → counts by command (success/fail)")
                                .arg(Arg::new("since").long("since").value_name("WINDOW")
                                     .help("24h | 7d | RFC3339 (inclusive)"))
                        )
                )
                // dev docs
                .subcommand(
                    Command::new("docs").about("Documentation utilities")
                        .subcommand(
                            Command::new("build")
                                .about("Generate CLI reference and Rules doc")
                                .arg(Arg::new("open").long("open").action(ArgAction::SetTrue))
                        )
                )
                // dev schemas
                .subcommand(
                    Command::new("schemas").about("Export contract JSON Schemas")
                        .subcommand(
                            Command::new("export")
                                .about("Write JSON Schemas to a directory")
                                .arg(Arg::new("out").long("out").required(true).value_name("DIR"))
                        )
                )
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            // ---------------- dev init ----------------
            Some(("init", m)) => {
                let dry_run = m.get_flag("dry-run");
                let yes = m.get_flag("yes");
                let with_config = m.get_flag("with-config");

                let root = std::env::current_dir()?;
                let dot = root.join(".nexoscli");
                let log_dir = dot.join("log");
                let manifest = dot.join("manifest.toml");
                let gitignore = root.join(".gitignore");
                let cfg = dot.join("config.toml");

                let mut actions: Vec<String> = vec![];
                if !dot.exists() { actions.push(fmt(&dot)); }
                if !log_dir.exists() { actions.push(fmt(&log_dir)); }
                if !manifest.exists() { actions.push(fmt(&manifest)); }
                if needs_gitignore_line(&gitignore, ".nexoscli/log/")? {
                    actions.push(fmt(&gitignore));
                }
                if with_config && !cfg.exists() {
                    actions.push(fmt(&cfg));
                }

                if dry_run && !yes {
                    let exit = ctx.ok("dev","init.plan", &serde_json::json!({ "actions": actions }));
                    return Ok(exit);
                }

                if !dot.exists() { fs::create_dir_all(&dot)?; }
                if !log_dir.exists() { fs::create_dir_all(&log_dir)?; }
                if !manifest.exists() {
                    let repo = root.file_name().and_then(|s| s.to_str()).unwrap_or("repo");
                    let ts = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
                    let content = format!("[project]\nname = \"{}\"\nversion = \"0.1.0\"\ncreated = \"{}\"\n", repo, ts);
                    fs::write(&manifest, content)?;
                }
                if needs_gitignore_line(&gitignore, ".nexoscli/log/")? {
                    append_gitignore_line(&gitignore, ".nexoscli/log/")?;
                }
                if with_config && !cfg.exists() {
                    let content = r#"
[output]
default = "json"

[telemetry]
enabled = false
endpoint = ""
"#.trim_start().to_string();
                    fs::write(&cfg, content)?;
                }

                let exit = ctx.ok("dev","init.apply", &serde_json::json!({ "actions": actions }));
                Ok(exit)
            }

            // ---------------- dev plugin new ----------------
            Some(("plugin", pm)) => {
                if let Some(("new", m)) = pm.subcommand() {
                    let name = m.get_one::<String>("name").unwrap().to_string();
                    let desc = m.get_one::<String>("desc").cloned().unwrap_or_else(|| "A NexOS CLI plugin".to_string());
                    let register = m.get_flag("register");
                    let dry_run = m.get_flag("dry-run");
                    let yes = m.get_flag("yes");

                    let run = span!(Level::INFO, "dev.plugin.new", plugin = name.as_str(), register, dry_run, yes);
                    let _g = run.enter();

                    let root = std::env::current_dir()?;
                    let plug_dir = root.join("plugins").join(&name);
                    let cargo_toml = plug_dir.join("Cargo.toml");
                    let src_dir = plug_dir.join("src");
                    let lib_rs = src_dir.join("lib.rs");
                    let readme = plug_dir.join("README.md");

                    let crate_name = format!("nexoscli-plugin-{}", name);
                    let crate_ident = format!("nexoscli_plugin_{}", name.replace('-', "_"));
                    let type_name = to_camel(&name) + "Plugin";

                    let mut actions: Vec<String> = vec![];
                    if !plug_dir.exists() { actions.push(fmt(&plug_dir)); }
                    if !src_dir.exists() { actions.push(fmt(&src_dir)); }
                    if !cargo_toml.exists() { actions.push(fmt(&cargo_toml)); }
                    if !lib_rs.exists() { actions.push(fmt(&lib_rs)); }
                    if !readme.exists() { actions.push(fmt(&readme)); }

                    // Registration actions
                    let root_cargo = root.join("Cargo.toml");
                    let bin_cargo = root.join("nexoscli-bin").join("Cargo.toml");
                    let bin_main = root.join("nexoscli-bin").join("src").join("main.rs");

                    if register {
                        if needs_workspace_member(&root_cargo, &format!("plugins/{}", name))? {
                            actions.push(fmt(&root_cargo));
                        }
                        if needs_bin_dep(&bin_cargo, &crate_name, &format!("../plugins/{}", name))? {
                            actions.push(fmt(&bin_cargo));
                        }
                        if needs_main_register(&bin_main, &crate_ident, &type_name)? {
                            actions.push(fmt(&bin_main));
                        }
                    }

                    if dry_run && !yes {
                        let exit = ctx.ok("dev","plugin.new.plan", &serde_json::json!({
                            "plugin": name,
                            "actions": actions,
                            "registered": register
                        }));
                        return Ok(exit);
                    }

                    // Apply
                    let mut created_paths = vec![];
                    if !plug_dir.exists() { fs::create_dir_all(&plug_dir)?; created_paths.push(pstr(&plug_dir)); }
                    if !src_dir.exists() { fs::create_dir_all(&src_dir)?; created_paths.push(pstr(&src_dir)); }
                    if !cargo_toml.exists() {
                        fs::write(&cargo_toml, plugin_cargo_toml(&crate_name))?;
                        created_paths.push(pstr(&cargo_toml));
                    }
                    if !lib_rs.exists() {
                        fs::write(&lib_rs, plugin_lib_rs(&type_name, &desc, &name))?;
                        created_paths.push(pstr(&lib_rs));
                    }
                    if !readme.exists() {
                        fs::write(&readme, plugin_readme(&name, &desc))?;
                        created_paths.push(pstr(&readme));
                    }

                    if register {
                        if needs_workspace_member(&root_cargo, &format!("plugins/{}", name))? {
                            add_workspace_member(&root_cargo, &format!("plugins/{}", name))?;
                        }
                        if needs_bin_dep(&bin_cargo, &crate_name, &format!("../plugins/{}", name))? {
                            add_bin_dep(&bin_cargo, &crate_name, &format!("../plugins/{}", name))?;
                        }
                        if needs_main_register(&bin_main, &crate_ident, &type_name)? {
                            add_main_register(&bin_main, &crate_ident, &type_name)?;
                        }
                    }

                    let exit = ctx.ok("dev","plugin.new.apply", &serde_json::json!({
                        "plugin": name,
                        "planned_actions": actions,
                        "created_paths": created_paths,
                        "registered": register
                    }));
                    return Ok(exit);
                }
                Ok(2)
            }

            // ---------------- dev plugins check ----------------
            Some(("plugins", pm)) => {
                if let Some(("check", m)) = pm.subcommand() {
                    let fix_readmes = m.get_flag("fix-readmes");
                    let root = std::env::current_dir()?;
                    let plugins_dir = root.join("plugins");
                    let bin_cargo = root.join("nexoscli-bin").join("Cargo.toml");
                    let bin_main = root.join("nexoscli-bin").join("src").join("main.rs");

                    let mut items = vec![];
                    if plugins_dir.exists() {
                        for entry in fs::read_dir(&plugins_dir)? {
                            let entry = entry?;
                            if !entry.file_type()?.is_dir() { continue; }
                            let pname = entry.file_name().to_string_lossy().to_string();
                            let pdir = entry.path();

                            let readme = pdir.join("README.md");
                            let cargo_toml = pdir.join("Cargo.toml");
                            let src_lib = pdir.join("src").join("lib.rs");

                            let mut check = PluginCheck{
                                plugin: pname.clone(),
                                has_readme: readme.exists(),
                                has_cargo_toml: cargo_toml.exists(),
                                has_src_lib: src_lib.exists(),
                                crate_name_ok: false,
                                bin_dep_ok: false,
                                main_registered: false,
                                errors: vec![],
                            };

                            // crate name convention
                            if cargo_toml.exists() {
                                let content = fs::read_to_string(&cargo_toml)?;
                                let okay = content.lines().any(|l| l.trim_start().starts_with("name = ") && l.contains(&format!("\"nexoscli-plugin-{}\"", pname)));
                                check.crate_name_ok = okay;
                                if !okay { check.errors.push("Cargo.toml name should be \"nexoscli-plugin-<dir>\"".into()); }
                            } else {
                                check.errors.push("missing Cargo.toml".into());
                            }

                            // bin dependency present?
                            let crate_name = format!("nexoscli-plugin-{}", pname);
                            if bin_cargo.exists() {
                                let bcontent = fs::read_to_string(&bin_cargo)?;
                                let dep_line = format!("{} = ", crate_name);
                                check.bin_dep_ok = bcontent.contains(&dep_line);
                                if !check.bin_dep_ok {
                                    check.errors.push("bin Cargo.toml missing dependency".into());
                                }
                            }

                            // main.rs registration present?
                            let crate_ident = format!("nexoscli_plugin_{}", pname.replace('-', "_"));
                            if bin_main.exists() {
                                let mcontent = fs::read_to_string(&bin_main)?;
                                check.main_registered = mcontent.contains(&crate_ident) && mcontent.contains("registry.register(");
                                if !check.main_registered {
                                    check.errors.push("main.rs missing registry.register for this plugin".into());
                                }
                            }

                            // fix README if asked
                            if fix_readmes && !check.has_readme {
                                fs::write(&readme, plugin_readme(&pname, "A NexOS CLI plugin"))?;
                                check.has_readme = true;
                            }
                            if !check.has_readme {
                                check.errors.push("missing README.md".into());
                            }
                            if !check.has_src_lib {
                                check.errors.push("missing src/lib.rs".into());
                            }

                            items.push(check);
                        }
                    }

                    let total = items.len();
                    let ok = items.iter().filter(|i| i.errors.is_empty()).count();
                    let not_ok = total - ok;
                    let summary = if not_ok == 0 { "all plugins OK" } else { "some plugins need fixes" }.to_string();
                    let result = PluginsCheckResult{ summary, total, ok, not_ok, items };

                    let exit = if not_ok == 0 {
                        ctx.ok("dev","plugins.check", &result)
                    } else {
                        // include details in data; mark envelope as error to signal CI
                        ctx.err("dev","plugins.check", "plugins.structure.invalid", "Some plugins failed checks", Some("Run with --fix-readmes or scaffold missing files"), 2)
                    };
                    return Ok(exit);
                }
                Ok(2)
            }

            // ---------------- dev doctor ----------------
            Some(("doctor", m)) => {
                if m.get_flag("ci") {
                    let cwd = std::env::current_dir()?;
                    let want: std::collections::HashSet<String> = m
                        .get_one::<String>("checks")
                        .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
                        .unwrap_or_else(|| ["graph","plugins","config","events","tools","cline"]
                            .into_iter().map(|s| s.to_string()).collect());

                    let mut checks = Vec::<DoctorCiCheck>::new();

                    // config
                    if want.contains("config") {
                        let cfg = &ctx.config;
                        checks.push(DoctorCiCheck {
                            name: "config_loaded".into(),
                            ok: true,
                            detail: format!("output.default={:?} telemetry.enabled={} endpoint={:?}",
                                cfg.output.default, cfg.telemetry.enabled, cfg.telemetry.endpoint),
                            severity: "info".into(),
                            fix: None,
                        });
                    }

                    // events: try to write a ping
                    if want.contains("events") {
                        let ok = ctx.events.write("dev","doctor.ci.ping",&serde_json::json!({"ping":true}),0,None,None,None).is_ok();
                        let p = std::path::PathBuf::from(".nexoscli/log/events.jsonl");
                        checks.push(DoctorCiCheck {
                            name: "event_log_writable".into(),
                            ok,
                            detail: p.display().to_string(),
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: if ok { None } else { Some("Ensure .nexoscli/log/ directory is writable".into()) },
                        });
                    }

                    // graph
                    if want.contains("graph") {
                        let (code, val) = run_cli_json(&cwd, &["graph","check"])?;
                        let ok = code == 0 && val.get("errors").and_then(|e| e.as_array()).map(|a| a.is_empty()).unwrap_or(true);
                        checks.push(DoctorCiCheck {
                            name: "graph_check".into(),
                            ok,
                            detail: if ok { "OK".into() } else { format!("errors: {}", val.get("errors").unwrap_or(&serde_json::json!(null))) },
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: if ok { None } else { Some("Run 'nexoscli graph check' to see specific issues".into()) },
                        });
                    }

                    // plugins
                    if want.contains("plugins") {
                        let (code, val) = run_cli_json(&cwd, &["dev","plugins","check"])?;
                        let ok = code == 0;
                        checks.push(DoctorCiCheck {
                            name: "plugins_check".into(),
                            ok,
                            detail: if ok { "OK".into() } else { format!("errors: {}", val.get("errors").unwrap_or(&serde_json::json!(null))) },
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: if ok { None } else { Some("Run 'nexoscli dev plugins check' to see specific plugin issues".into()) },
                        });
                    }

                    // tools
                    if want.contains("tools") {
                        let (code, val) = run_cli_json(&cwd, &["tools","export"])?;
                        let ok = code == 0 && val["data"]["tools"].as_array().map(|a| !a.is_empty()).unwrap_or(false);
                        let n = val["data"]["tools"].as_array().map(|a| a.len()).unwrap_or(0);
                        checks.push(DoctorCiCheck {
                            name: "tools_export".into(),
                            ok,
                            detail: format!("tools={}", n),
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: None,
                        });
                    }

                    // cline (informational unless required)
                    if want.contains("cline") {
                        let mut ok = true;
                        let mut detail = String::new();
                        let mut fix = None;

                        // call cline locate
                        let (code, val) = run_cli_json(&cwd, &["cline","locate"])?;
                        if code != 0 {
                            ok = false;
                            detail = "cline locate failed".into();
                        } else {
                            let sel = val["data"]["selected"].as_str();
                            if sel.is_none() {
                                ok = false;
                                detail = "no Cline storage found".into();
                                fix = Some("If in Dev Container, mount your host globalStorage; or set NEXOS_CLINE_STORAGE=/path/to/.../tasks".into());
                            } else {
                                // dry-run import of a few recent files
                                let storage = sel.unwrap();
                                let (c2, v2) = run_cli_json(&cwd, &["cline","import","--storage",storage,"--since","7d","--limit","5","--dry-run"])?;
                                ok = ok && (c2 == 0);
                                if ok {
                                    let imp = v2["data"]["imported"].as_u64().unwrap_or(0);
                                    detail = format!("storage={} imported~{}", storage, imp);
                                } else {
                                    detail = format!("import dry-run failed for {}", storage);
                                    fix = Some("Inspect storage path; ensure readable JSON files under tasks/".into());
                                }
                            }
                        }

                        checks.push(DoctorCiCheck {
                            name: "cline".into(),
                            ok,
                            detail,
                            severity: if ok { "info".into() } else { "info".into() }, // soft by default
                            fix,
                        });
                    }

                    // rules (soft by default)
                    if want.contains("rules") {
                        let (code_lint, _v1) = run_cli_json(&cwd, &["rules","lint"])?;
                        let ok_lint = code_lint == 0;
                        // also ensure export works for cline & human
                        let (code_ec, _v2) = run_cli_json(&cwd, &["rules","export","--for","cline","--out",".clinerules/compiled.md"])?;
                        let (code_eh, _v3) = run_cli_json(&cwd, &["rules","export","--for","human","--out","docs/RULES.md"])?;
                        let ok = ok_lint && code_ec == 0 && code_eh == 0;
                        checks.push(DoctorCiCheck{
                            name: "rules".into(),
                            ok,
                            detail: if ok { "lint & export OK".into() } else { "rules lint/export failed".into() },
                            severity: "info".into(),
                            fix: Some("Run `nexoscli rules init` then `nexoscli rules lint`".into()),
                        });
                    }

                    // Determine required checks (via flag or env)
                    let mut required: std::collections::HashSet<String> = m
                        .get_one::<String>("require")
                        .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
                        .unwrap_or_default();
                    if std::env::var("NEXOS_CLINE_REQUIRED").ok().as_deref() == Some("1") {
                        required.insert("cline".into());
                    }

                    // Graph and plugins are critical by default
                    let critical_by_default = ["graph_check", "plugins_check"];
                    
                    let ok_all = checks.iter().all(|c| {
                        if required.contains(&c.name) || critical_by_default.contains(&c.name.as_str()) { 
                            c.ok 
                        } else { 
                            true 
                        }
                    });
                    
                    if ok_all {
                        let exit = ctx.ok("dev","doctor.ci",&DoctorCiResult{ ok: true, checks });
                        return Ok(exit);
                    } else {
                        // include failing required and critical by default checks in message; exit non-zero
                        let failing: Vec<_> = checks.iter().filter(|c| {
                            (required.contains(&c.name) || critical_by_default.contains(&c.name.as_str())) && !c.ok
                        }).map(|c| c.name.clone()).collect();
                        let exit = ctx.err("dev","doctor.ci","doctor.ci.failed",
                            &format!("Critical checks failed: {}", failing.join(",")),
                            Some("Run with --checks to inspect sub-checks; see devcontainer mounts for cline"),
                            2);
                        return Ok(exit);
                    }
                } else {
                    let since = m.get_one::<String>("since").unwrap();
                    let require = m.get_one::<String>("require").unwrap().split(',').filter(|s|!s.is_empty()).collect::<Vec<_>>();
                    let min_cov = parse_cov_threshold(m.get_one::<String>("task_coverage_min").unwrap());
                    let _outfmt = m.get_one::<String>("output").unwrap();

                    let mut report = serde_json::Map::new();
                    let mut hard_fail = false;

                    // 1) Redaction config present & valid (hard requirement by default)
                    let red_ok = crate::checks::redaction_ok();
                    report.insert("redaction".into(), serde_json::json!({ "ok": red_ok }));
                    if !red_ok { 
                        hard_fail = true; 
                    }

                    // 2) Event integrity & counts for window
                    let ev = crate::checks::events_window(since)?;
                    report.insert("events".into(), serde_json::to_value(&ev)?);
                    if (!ev.ok) && require.iter().any(|c| *c == "events") { hard_fail = true; }

                    // 3) Task coverage
                    let cov = if ev.total_lines > 0 { ev.with_task as f64 / ev.total_lines as f64 } else { 1.0 };
                    let task_cov_ok = cov >= min_cov;
                    report.insert("task_coverage".into(), serde_json::json!({ "coverage": cov, "min": min_cov, "ok": task_cov_ok }));
                    
                    // Check if --task-coverage-min was explicitly set by user
                    let cov_explicitly_set = m.value_source("task_coverage_min") == Some(ValueSource::CommandLine);
                    
                    if !task_cov_ok && (cov_explicitly_set || require.iter().any(|c| *c == "tasks" || *c == "coverage")) { 
                        hard_fail = true; 
                    }

                    // Add overall ready status for basic usage
                    report.insert("ready".into(), serde_json::json!(!hard_fail));

                    let code = if hard_fail { ctx.err("dev","doctor","DOCTOR_FAILED","one or more required checks failed",None,2) } else { ctx.ok("dev","doctor",&report) };
                    return Ok(code);
                }
            }

            // ---------------- dev telemetry ----------------
            Some(("telemetry", tm)) => {
                if let Some(("report", m)) = tm.subcommand() {
                    let since_s = m.get_one::<String>("since").cloned();
                    let since_ts: Option<OffsetDateTime> = match since_s.as_deref() {
                        None => None,
                        Some(s) if s.ends_with("h") => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
                        Some(s) if s.ends_with("d") => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
                        Some(s) => OffsetDateTime::parse(s, &Rfc3339).ok(),
                    };

                    let log = std::env::current_dir()?.join(".nexoscli").join("log").join("events.jsonl");
                    let mut rows = Vec::<serde_json::Value>::new();
                    if log.exists() {
                        for line in std::fs::read_to_string(&log)?.lines() {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                                let ts_ok = match (&since_ts, v.get("ts").and_then(|x| x.as_str())) {
                                    (None, _) => true,
                                    (Some(since), Some(ts)) => OffsetDateTime::parse(ts, &Rfc3339).ok().map(|t| t >= *since).unwrap_or(true),
                                    _ => true,
                                };
                                if ts_ok { rows.push(v); }
                            }
                        }
                    }

                    use std::collections::{BTreeMap, HashMap};
                    
                    // per (plugin,command)
                    let mut by_durs: HashMap<(String,String), Vec<u64>> = HashMap::new();
                    let mut by_counts: HashMap<(String,String), (u64,u64,u64)> = HashMap::new(); // count, success, fail
                    let mut errors: HashMap<String, u64> = HashMap::new();

                    for v in &rows {
                        let p = v.get("plugin").and_then(|x| x.as_str()).unwrap_or("-").to_string();
                        let c = v.get("command").and_then(|x| x.as_str()).unwrap_or("-").to_string();
                        let code = v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0);
                        let dur = v.get("dur_ms").and_then(|x| x.as_u64());

                        let e = by_counts.entry((p.clone(), c.clone())).or_insert((0,0,0));
                        e.0 += 1;
                        if code == 0 { e.1 += 1; } else { e.2 += 1; }

                        if let Some(ms) = dur {
                            by_durs.entry((p.clone(), c.clone())).or_default().push(ms);
                        }

                        if let Some(ec) = v.get("error_code").and_then(|x| x.as_str()) {
                            if !ec.is_empty() { *errors.entry(ec.to_string()).or_insert(0) += 1; }
                        }
                    }

                    // build rows with percentiles
                    let mut by_command: Vec<TelemetryByCommand> = Vec::new();
                    for (key, (cnt, ok, fail)) in by_counts.into_iter() {
                        let durs = by_durs.remove(&key).unwrap_or_default();
                        let mut durs_sorted = durs.clone();
                        durs_sorted.sort_unstable();
                        let row = TelemetryByCommand {
                            plugin: key.0,
                            command: key.1,
                            count: cnt, success: ok, fail,
                            p50_ms: percentile(&durs_sorted, 50.0),
                            p90_ms: percentile(&durs_sorted, 90.0),
                            avg_ms:  average(&durs_sorted),
                        };
                        by_command.push(row);
                    }
                    by_command.sort_by(|a,b| b.count.cmp(&a.count));

                    // error leaderboard
                    let mut errors_by_code: Vec<ErrorCodeRow> = errors.into_iter()
                        .map(|(k,v)| ErrorCodeRow{ error_code: k, count: v })
                        .collect();
                    errors_by_code.sort_by(|a,b| b.count.cmp(&a.count));

                    // slowest by p90
                    let mut slow: Vec<SlowRow> = by_command.iter()
                        .filter_map(|r| r.p90_ms.map(|p| SlowRow{ plugin: r.plugin.clone(), command: r.command.clone(), p90_ms: p, count: r.count }))
                        .collect();
                    slow.sort_by(|a,b| b.p90_ms.cmp(&a.p90_ms).then(b.count.cmp(&a.count)));
                    if slow.len() > 5 { slow.truncate(5); }

                    let r = TelemetryReport {
                        total_events: rows.len() as u64,
                        window_start: since_ts.map(|t| t.format(&Rfc3339).unwrap()),
                        by_command,
                        errors_by_code,
                        slowest_commands: slow,
                    };
                    let exit = ctx.ok("dev","telemetry.report",&r);
                    return Ok(exit);
                }
                Ok(2)
            }

            // ---------------- dev docs ----------------
            Some(("docs", dm)) => {
                if let Some(("build", m)) = dm.subcommand() {
                    let cwd = std::env::current_dir()?;
                    // 1) Top-level help
                    let (_c1, help) = run_cli_text(&cwd, &["--help"])?;
                    // 2) tools export (machine specs)
                    let (_c2, tools) = run_cli_json(&cwd, &["tools","export"])?;
                    // 3) rules export (humans)
                    let (_c3, _v3) = run_cli_json(&cwd, &["rules","export","--for","human","--out","docs/RULES.md"])?;

                    fs::create_dir_all("docs")?;
                    let mut md = String::new();
                    md.push_str("# NEXOS CLI Reference (generated)\n\n");
                    md.push_str("## Top-level Help\n\n```\n");
                    md.push_str(&help);
                    md.push_str("\n```\n\n");

                    md.push_str("## Tool Specs (for agents)\n\n");
                    let tools_arr = tools["data"]["tools"].as_array().cloned().unwrap_or_default();
                    for t in tools_arr {
                        let name = t["name"].as_str().unwrap_or("-");
                        let about = t["about"].as_str().unwrap_or("-");
                        md.push_str(&format!("### `{}`\n\n{}\n\n", name, about));
                    }
                    fs::write("docs/CLI_REFERENCE.md", md.as_bytes())?;

                    // Optionally open
                    if m.get_flag("open") {
                        #[cfg(target_os="macos")] { let _ = std::process::Command::new("open").arg("docs/CLI_REFERENCE.md").status(); }
                        #[cfg(target_os="linux")] { let _ = std::process::Command::new("xdg-open").arg("docs/CLI_REFERENCE.md").status(); }
                    }

                    let out = serde_json::json!({"written": ["docs/CLI_REFERENCE.md", "docs/RULES.md"]});
                    let code = ctx.ok("dev","docs.build",&out);
                    return Ok(code);
                }
                Ok(2)
            }

            // ---------------- dev schemas ----------------
            Some(("schemas", sm)) => {
                if let Some(("export", esm)) = sm.subcommand() {
                    let out = esm.get_one::<String>("out").unwrap();
                    let dir = std::path::Path::new(out);
                    std::fs::create_dir_all(dir)?;

                    // 1) Event (from stable EventLineV03 contract type)
                    let ev = schema_for!(nexoscli_core::contracts::EventLineV03);
                    std::fs::write(dir.join("event_v0.3.schema.json"), serde_json::to_string_pretty(&ev)?)?;

                    // 2) Term record
                    let tr = schema_for!(nexoscli_core::contracts::TermRecordV01);
                    std::fs::write(dir.join("term_record_v0.1.schema.json"), serde_json::to_string_pretty(&tr)?)?;

                    // 3) File watch event
                    let fw = schema_for!(nexoscli_core::contracts::FileWatchEventV01);
                    std::fs::write(dir.join("file_watch_event_v0.1.schema.json"), serde_json::to_string_pretty(&fw)?)?;

                    // 4) Bag manifest
                    let bm = schema_for!(nexoscli_core::contracts::BagManifestV02);
                    std::fs::write(dir.join("bag_manifest_v0.2.schema.json"), serde_json::to_string_pretty(&bm)?)?;

                    let exit = ctx.ok("dev","schemas.export",
                        &serde_json::json!({"out": out, "files":[
                            "event_v0.3.schema.json",
                            "term_record_v0.1.schema.json",
                            "file_watch_event_v0.1.schema.json",
                            "bag_manifest_v0.2.schema.json"
                        ]})
                    );
                    return Ok(exit);
                }
                Ok(2)
            }

            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            // dev init
            ToolSpec {
                name: "dev.init".to_string(),
                about: "Initialize .nexoscli/ (Plan/Apply, idempotent)".to_string(),
                args_schema: schema_for!(DevInitArgs),
                result_schema: schema_for!(DevInitResult),
            },
            // plugin new
            ToolSpec {
                name: "dev.plugin.new".to_string(),
                about: "Scaffold a new plugin and optionally register it".to_string(),
                args_schema: schema_for!(PluginNewArgs),
                result_schema: schema_for!(PluginNewResult),
            },
            // plugins check
            ToolSpec {
                name: "dev.plugins.check".to_string(),
                about: "Verify plugin folder structure and registration".to_string(),
                args_schema: schema_for!(PluginsCheckArgs),
                result_schema: schema_for!(PluginsCheckResult),
            },
            // doctor
            ToolSpec {
                name: "dev.doctor".to_string(),
                about: "Verify local setup (dotdir, event log write, config, OTEL envs); --ci aggregates checks".to_string(),
                args_schema: schema_for!(DoctorArgs),
                result_schema: schema_for!(DoctorCiResult),
            },
            // telemetry report
            ToolSpec {
                name: "dev.telemetry.report".to_string(),
                about: "Aggregate events.jsonl → counts, p50/p90/avg, error leaderboard".to_string(),
                args_schema: schema_for!(TelemetryReportArgs),
                result_schema: schema_for!(TelemetryReport),
            },
        ]
    }
}

// ===== helpers =====

fn fmt<T: AsRef<Path>>(s: T) -> String { s.as_ref().display().to_string() }
fn pstr(p: &PathBuf) -> String { p.display().to_string() }

// Percentile and average calculations
fn percentile(sorted: &Vec<u64>, pct: f64) -> Option<u64> {
    if sorted.is_empty() { return None; }
    let n = sorted.len() as f64;
    let idx = (pct * (n - 1.0) / 100.0).round() as usize;
    sorted.get(idx).copied()
}

fn average(v: &Vec<u64>) -> Option<u64> {
    if v.is_empty() { return None; }
    let s: u128 = v.iter().map(|&x| x as u128).sum();
    Some((s / (v.len() as u128)) as u64)
}

fn parse_cov_threshold(s: &str) -> f64 {
    let t = s.trim();
    // Allow "50%" style
    if let Some(num) = t.strip_suffix('%') {
        if let Ok(v) = num.trim().parse::<f64>() {
            return (v / 100.0).clamp(0.0, 1.0);
        }
    }
    // Allow integers as percent (50 -> 0.5), or fractions (0.7)
    match t.parse::<f64>() {
        Ok(v) if v > 1.0 => (v / 100.0).clamp(0.0, 1.0),
        Ok(v)            => v.clamp(0.0, 1.0),
        Err(_)           => 0.7,
    }
}

fn to_camel(s: &str) -> String {
    let mut out = String::new();
    for part in s.split(|c: char| c == '-' || c == '_' || c.is_whitespace()) {
        if part.is_empty() { continue; }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push_str(&first.to_uppercase().to_string());
            out.push_str(&chars.as_str().to_lowercase());
        }
    }
    out
}

fn needs_gitignore_line(path: &PathBuf, line: &str) -> std::io::Result<bool> {
    if !path.exists() { return Ok(true); }
    let content = std::fs::read_to_string(path)?;
    Ok(!content.lines().any(|l| l.trim() == line.trim()))
}
fn append_gitignore_line(path: &PathBuf, line: &str) -> std::io::Result<()> {
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    if needs_gitignore_line(path, line)? {
        if f.metadata()?.len() > 0 { write!(f, "\n")?; }
        writeln!(f, "{}", line)?;
    }
    Ok(())
}

// ---- registration patch helpers ----

fn needs_workspace_member(root_cargo: &PathBuf, member: &str) -> std::io::Result<bool> {
    let content = fs::read_to_string(root_cargo)?;
    Ok(!content.contains(member))
}
fn add_workspace_member(root_cargo: &PathBuf, member: &str) -> std::io::Result<()> {
    let mut content = fs::read_to_string(root_cargo)?;
    // naive but robust: find "members = [" and insert before closing ]
    if let Some(start) = content.find("members") {
        if let Some(open) = content[start..].find('[') {
            let idx_open = start + open + 1;
            if let Some(close_rel) = content[idx_open..].find(']') {
                let idx_close = idx_open + close_rel;
                let to_insert = if content[idx_open..idx_close].trim().is_empty() {
                    format!("\n    \"{}\",\n", member)
                } else {
                    format!("    \"{}\",\n", member)
                };
                content.insert_str(idx_close, &to_insert);
                fs::write(root_cargo, content)?;
                return Ok(());
            }
        }
    }
    // fallback: append a new members block (if malformed)
    let append = format!("\n[workspace]\nmembers = [\n    \"{}\",\n]\n", member);
    fs::write(root_cargo, content + &append)?;
    Ok(())
}

fn needs_bin_dep(bin_cargo: &PathBuf, crate_name: &str, rel_path: &str) -> std::io::Result<bool> {
    let content = fs::read_to_string(bin_cargo)?;
    Ok(!content.contains(&format!("{} = ", crate_name)) || !content.contains(rel_path))
}
fn add_bin_dep(bin_cargo: &PathBuf, crate_name: &str, rel_path: &str) -> std::io::Result<()> {
    let mut content = fs::read_to_string(bin_cargo)?;
    let dep_line = format!("{} = {{ path = \"{}\" }}\n", crate_name, rel_path);
    if let Some(pos) = content.find("[dependencies]") {
        // insert after the [dependencies] header
        let insert_at = pos + "[dependencies]".len();
        content.insert_str(insert_at, &format!("\n{}\n", dep_line));
        fs::write(bin_cargo, content)?;
        Ok(())
    } else {
        // append a dependencies block
        content.push_str("\n[dependencies]\n");
        content.push_str(&dep_line);
        fs::write(bin_cargo, content)?;
        Ok(())
    }
}

fn needs_main_register(bin_main: &PathBuf, crate_ident: &str, type_name: &str) -> std::io::Result<bool> {
    let content = fs::read_to_string(bin_main)?;
    let needle = format!("{}.{}::new()", crate_ident, type_name);
    Ok(!content.contains(&needle))
}
fn add_main_register(bin_main: &PathBuf, crate_ident: &str, type_name: &str) -> std::io::Result<()> {
    let mut content = fs::read_to_string(bin_main)?;
    // Find last "registry.register(" and insert after; else after registry creation
    let needle = "registry.register(";
    if let Some(mut last) = content.rmatch_indices(needle).next().map(|(i, _)| i) {
        // find end of line
        if let Some(line_end) = content[last..].find('\n') {
            last = last + line_end + 1;
            let line = format!("    registry.register(Box::new({}::{}::new()));\n", crate_ident, type_name);
            content.insert_str(last, &line);
            fs::write(bin_main, content)?;
            return Ok(());
        }
    }
    // fallback: find creation of registry
    if let Some(pos) = content.find("let mut registry = PluginRegistry::new();") {
        let insert_at = pos + "let mut registry = PluginRegistry::new();".len();
        let line = format!("\n    registry.register(Box::new({}::{}::new()));\n", crate_ident, type_name);
        content.insert_str(insert_at, &line);
        fs::write(bin_main, content)?;
        return Ok(());
    }
    // if neither, append at top-level (least ideal, but safe)
    let line = format!("\n// added by dev plugin scaffold\n// ensure PluginRegistry exists above\n// {}\n", format!("registry.register(Box::new({}::{}::new()));", crate_ident, type_name));
    content.push_str(&line);
    fs::write(bin_main, content)?;
    Ok(())
}

// ---- file templates ----

fn plugin_cargo_toml(crate_name: &str) -> String {
    format!(r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = {{ path = "../../nexoscli-core" }}
anyhow = {{ workspace = true }}
serde = {{ workspace = true }}
serde_json = {{ workspace = true }}
schemars = {{ workspace = true }}
clap = {{ workspace = true }}
time = {{ workspace = true }}
"#)
}

fn plugin_lib_rs(type_name: &str, desc: &str, short_name: &str) -> String {
    format!(r#"use clap::{{Arg, ArgMatches, Command}};
use schemars::{{schema_for, JsonSchema}};
use serde::{{Deserialize, Serialize}};

use nexoscli_core::plugin::{{CommandContext, Plugin, ToolSpec}};
use nexoscli_core::Result;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct HelloArgs {{
    name: Option<String>,
}}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct HelloResult {{
    message: String,
}}

pub struct {type_name};
impl {type_name} {{
    pub fn new() -> Self {{ Self }}
}}

impl Plugin for {type_name} {{
    fn name(&self) -> &'static str {{ "{short_name}" }}
    fn version(&self) -> &'static str {{ env!("CARGO_PKG_VERSION") }}
    fn describe(&self) -> &'static str {{ "{desc}" }}

    fn augment_commands(&self, app: Command) -> Command {{
        app.subcommand(
            Command::new("{short_name}")
              .about("{desc}")
              .subcommand(
                 Command::new("hello")
                   .about("Say hello from {short_name} plugin")
                   .arg(Arg::new("name").long("name").required(false).value_name("TEXT"))
              )
        )
    }}

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {{
        match sub_matches.subcommand() {{
            Some(("hello", m)) => {{
                let name = m.get_one::<String>("name").cloned().unwrap_or_else(|| "world".to_string());
                let data = HelloResult {{ message: format!("hello, {{}}", name) }};
                let exit = ctx.ok("{short_name}", "hello", &serde_json::json!({{ "message": data.message }}));
                Ok(exit)
            }}
            _ => Ok(2),
        }}
    }}

    fn tool_specs(&self) -> Vec<ToolSpec> {{
        vec![ToolSpec {{
            name: "{short_name}.hello".to_string(),
            about: "Say hello".to_string(),
            args_schema: schema_for!(HelloArgs),
            result_schema: schema_for!(HelloResult),
        }}]
    }}
}}
"#)
}

fn plugin_readme(name: &str, desc: &str) -> String {
    format!(r#"# {name} plugin

{desc}

## Commands

- `{name} hello [--name TEXT]` — say hello

## Implementation Notes

- This plugin implements `Plugin` with `name() = "{name}"`.
- Add/rename commands in `augment_commands()` and route in `execute()`.
- Keep outputs using the core envelope (`ctx.ok / ctx.err`) to stay machine-friendly.

"#)
}

// ===== Phase 7 helpers =====

fn run_cli_json(cwd: &Path, args: &[&str]) -> anyhow::Result<(i32, serde_json::Value)> {
    let exe = std::env::current_exe()?;
    let mut a: Vec<&str> = args.to_vec();
    if !a.iter().any(|x| *x == "--output") {
        a.push("--output"); a.push("json");
    }
    let out = Proc::new(exe).current_dir(cwd).args(&a).output()?;
    let code = out.status.code().unwrap_or(-1);
    let val: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| anyhow::anyhow!("stdout not JSON for {:?}: {}", a, e))?;
    Ok((code, val))
}

fn run_cli_text(cwd: &Path, args: &[&str]) -> anyhow::Result<(i32, String)> {
    let exe = std::env::current_exe()?;
    let out = Proc::new(exe).current_dir(cwd).args(args).output()?;
    let code = out.status.code().unwrap_or(-1);
    Ok((code, String::from_utf8_lossy(&out.stdout).to_string()))
}
