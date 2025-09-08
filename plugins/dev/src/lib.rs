use clap::{Arg, ArgAction, ArgMatches, Command};
use clap::parser::ValueSource;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as Proc;
use std::ffi::OsStr;
use time::{format_description::well_known::Rfc3339, OffsetDateTime, Duration};
use tracing::{span, Level};
use serde_yaml;

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

mod checks;
mod repo;
mod cmd_log;
mod cmd_docs;
pub mod cmd_init;


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
    #[serde(default)]
    strict: bool,
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
    #[serde(default)]
    has_tests_dir: bool,
    #[serde(default)]
    readme_has_commands_auto: bool,
    #[serde(default)]
    src_loc_warn_over: u64,
    #[serde(default)]
    src_loc_err_over: u64,
    #[serde(default)]
    files_over_warn: Vec<String>,
    #[serde(default)]
    files_over_err: Vec<String>,
    #[serde(default)]
    warnings: Vec<String>,
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStartArgs {
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStartResult {
    picked_plan: String,
    moved_from: String,
    moved_to: String,
    task_id: String,
    title: String,
    plan_id: String,
    slug: String,
    updated_frontmatter: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanFinalizeArgs {
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    bundle_out: Option<String>,
    #[serde(default)]
    implemented_in: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanFinalizeResult {
    plan: String,
    moved_to: String,
    task_id: Option<String>,
    implemented_in: String,
    bundle_out: Option<String>,
    frontmatter: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanPauseArgs {
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanPauseResult {
    plan: String,
    moved_to: String,
    #[serde(default)]
    rationale: Option<String>,
    frontmatter: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanResumeArgs {
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanResumeResult {
    plan: String,
    moved_to: String,
    current_was_empty: bool,
    reconciliation: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanArchiveArgs {
    plan: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanArchiveResult {
    plan: String,
    moved_to: String,
    #[serde(default)]
    rationale: Option<String>,
    frontmatter: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanDumpArgs {
    plan: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanDumpResult {
    plan: String,
    moved_to: String,
    #[serde(default)]
    rationale: Option<String>,
    frontmatter: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanRequeueArgs {
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanRequeueResult {
    plan: String,
    moved_to: String,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanListArgs {
    #[serde(default)]
    r#where: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanRow {
    folder: String,
    file: String,
    plan_id: Option<String>,
    slug: Option<String>,
    title: Option<String>,
    status: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanListResult {
    total: usize,
    items: Vec<PlanRow>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStatusArgs {
    plan: String,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStatusResult {
    plan: String,
    frontmatter: serde_json::Value,
    title: Option<String>,
    plan_id: Option<String>,
    slug: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlansVerifyArgs {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlansVerifyItem {
    name: String,
    ok: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
    #[serde(default)]
    touched: Vec<String>,
    #[serde(default)]
    count: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlansVerifyResult {
    ok: bool,
    summary: String,
    items: Vec<PlansVerifyItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlansIndexArgs {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlansIndexResult {
    ok: bool,
    touched: Vec<String>,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlansIndexBuildArgs {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    dry_run: bool,
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
                .subcommand(cmd_init::subcommand())
                // dev plugin new
                .subcommand(
                    Command::new("plugin")
                        .about("Plugin utilities")
                        .subcommand(
                            Command::new("new")
                                .about("Scaffold a new plugin under plugins/<name>")
                                .arg(Arg::new("name").long("name").required(true).value_name("NAME"))
                                .arg(Arg::new("desc").long("desc").required(false).value_name("TEXT"))
                                .arg(Arg::new("blueprint").long("blueprint").required(false).value_parser(["v1"]).help("Use external blueprint templates"))
                                .arg(Arg::new("tpl-dir").long("tpl-dir").required(false).value_name("PATH").help("Template directory (defaults to templates/plugin/blueprint-v1)"))
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
                                .arg(Arg::new("strict").long("strict").action(ArgAction::SetTrue).help("Exit non-zero if any plugin has structural errors"))
                        )
                )
                // dev repo
                .subcommand(
                    Command::new("repo").about("Repository entropy & structure safeguards")
                      .subcommand(
                         Command::new("check").about("Validate structure against policy")
                           .arg(Arg::new("policy").long("policy").required(false).value_name("PATH"))
                           .arg(Arg::new("fix-readmes").long("fix-readmes").action(ArgAction::SetTrue)
                                .help("Auto-create missing READMEs according to policy"))
                      )
                      .subcommand(
                         Command::new("policy").about("Manage repo policy")
                           .subcommand(
                             Command::new("init").about("Write default .nexoscli/repo_policy.yaml")
                               .arg(Arg::new("force").long("force").action(ArgAction::SetTrue))
                           )
                      )
                      .subcommand(
                         Command::new("readmes").about("Scaffold missing folder READMEs")
                           .arg(Arg::new("policy").long("policy").required(false).value_name("PATH"))
                           .arg(Arg::new("apply").long("apply").action(ArgAction::SetTrue))
                      )
                )
                // dev log
                .subcommand(cmd_log::subcommand())
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
                        .arg(Arg::new("profile").long("profile").value_parser(["v0"]).help("Preselect checks and required gates; v0 = stable CI defaults"))
                        .arg(Arg::new("checks").long("checks").value_name("CSV")
                             .help("Subset of checks: graph,plugins,config,events,tools,events-schema,events-chain,events-rotation,repo,plans,cline,rules (default: all)"))
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
                        .subcommand(
                            Command::new("hai")
                                .about("Aggregate AI-interaction metrics from AIM streams (.nexoscli/agents/streams/*)")
                                .arg(Arg::new("since").long("since").value_name("WINDOW"))
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
                        .subcommand(cmd_docs::subcommand())
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
                // dev plans (project plan lifecycle)
                .subcommand(
                    Command::new("plans").about("Project plan lifecycle")
                      .subcommand(
                        Command::new("list").about("List plans")
                          .arg(Arg::new("where").long("where").value_parser(["next","current","done","archived","all"]).default_value("all"))
                      )
                      .subcommand(
                        Command::new("next").about("Show the next plan candidate under docs/project/next")
                      )
                      .subcommand(
                        Command::new("status").about("Show plan front-matter and derived ids")
                          .arg(Arg::new("plan").long("plan").required(true).value_name("PATH"))
                      )
                      .subcommand(
                        Command::new("start").about("Start a plan: create+start task, update front-matter, move next→current")
                          .arg(Arg::new("plan").long("plan").required(false).value_name("PATH"))
                          .arg(Arg::new("scope").long("scope").required(false).value_name("LABEL"))
                          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                      )
                      .subcommand(
                        Command::new("finalize").about("Finalize a plan: mark done, end task, bundle, move current→done")
                          .arg(Arg::new("plan").long("plan").required(false).value_name("PATH"))
                          .arg(Arg::new("bundle-out").long("bundle-out").required(false).value_name("PATH"))
                          .arg(Arg::new("implemented-in").long("implemented-in").required(false).value_name("SHA"))
                          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                      )
                      .subcommand(
                        Command::new("pause").about("Pause current plan: move current→paused")
                          .arg(Arg::new("plan").long("plan").required(false).value_name("PATH"))
                          .arg(Arg::new("reason").long("reason").required(false).value_name("TEXT"))
                          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                      )
                      .subcommand(
                        Command::new("resume").about("Resume a paused plan: move paused→current")
                          .arg(Arg::new("plan").long("plan").required(false).value_name("PATH"))
                          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                      )
                      .subcommand(
                        Command::new("archive").about("Archive a plan: move to archived/")
                          .arg(Arg::new("plan").long("plan").required(true).value_name("PATH"))
                          .arg(Arg::new("reason").long("reason").required(false).value_name("TEXT"))
                          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                      )
                      .subcommand(
                        Command::new("dump").about("Dump a plan: move to dumped/")
                          .arg(Arg::new("plan").long("plan").required(true).value_name("PATH"))
                          .arg(Arg::new("reason").long("reason").required(false).value_name("TEXT"))
                          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                      )
                      .subcommand(
                        Command::new("requeue").about("Requeue a paused plan to backlog/")
                          .arg(Arg::new("plan").long("plan").required(false).value_name("PATH"))
                          .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                      )
                      .subcommand(
                        Command::new("verify").about("Validate plan lifecycle folder rules")
                          .arg(Arg::new("path").long("path").required(false).value_name("DIR"))
                      )
                      .subcommand(
                        Command::new("index").about("Indexes for plan READMEs")
                          .subcommand(
                            Command::new("build").about("Refresh AUTO (id=plans-index) blocks in docs/project/* READMEs")
                              .arg(Arg::new("path").long("path").required(false).value_name("DIR"))
                              .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                          )
                      )
                )
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            // ---------------- dev init ----------------
            Some(("init", m)) => {
                cmd_init::run(m, ctx)
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

                    // Support external blueprint templates
                    let blueprint = m.get_one::<String>("blueprint").cloned();
                    let tpl_dir = m.get_one::<String>("tpl-dir").cloned();
                    if blueprint.as_deref() == Some("v1") {
                        let run2 = span!(Level::INFO, "dev.plugin.new.blueprint", plugin = name.as_str(), register, dry_run, yes);
                        let _g2 = run2.enter();

                        let root = std::env::current_dir()?;
                        let tpl_base = tpl_dir
                            .map(|s| PathBuf::from(s))
                            .unwrap_or(root.join("templates").join("plugin").join("blueprint-v1"));
                        let plug_dir = root.join("plugins").join(&name);

                        let crate_name = format!("nexoscli-plugin-{}", name);
                        let crate_ident = format!("nexoscli_plugin_{}", name.replace('-', "_"));
                        let type_name = to_camel(&name) + "Plugin";

                        let files = collect_template_files(&tpl_base)?;
                        let mut actions: Vec<String> = Vec::new();
                        for src in &files {
                            let rel = src.strip_prefix(&tpl_base).unwrap();
                            actions.push(plug_dir.join(rel).display().to_string());
                        }

                        if dry_run && !yes {
                            let exit = ctx.ok("dev","plugin.new.plan", &serde_json::json!({
                                "plugin": name,
                                "actions": actions,
                                "registered": register
                            }));
                            return Ok(exit);
                        }

                        // Apply copy with token substitution
                        let mut created_paths: Vec<String> = vec![];
                        for src in &files {
                            let rel = src.strip_prefix(&tpl_base).unwrap();
                            let dst = plug_dir.join(rel);
                            if let Some(parent) = dst.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            let content = std::fs::read_to_string(src)?;
                            let mut tokens = std::collections::HashMap::new();
                            tokens.insert("plugin_name", name.clone());
                            tokens.insert("crate_name", crate_name.clone());
                            tokens.insert("crate_ident", crate_ident.clone());
                            tokens.insert("type_name", type_name.clone());
                            tokens.insert("desc", desc.clone());
                            let rendered = substitute_tokens(&content, &tokens);
                            std::fs::write(&dst, rendered.as_bytes())?;
                            created_paths.push(dst.display().to_string());
                        }

                        // Optional registration
                        if register {
                            let root_cargo = root.join("Cargo.toml");
                            let bin_cargo = root.join("nexoscli-bin").join("Cargo.toml");
                            let bin_main = root.join("nexoscli-bin").join("src").join("main.rs");

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
                                has_tests_dir: false,
                                readme_has_commands_auto: false,
                                src_loc_warn_over: 1000,
                                src_loc_err_over: 2000,
                                files_over_warn: vec![],
                                files_over_err: vec![],
                                warnings: vec![],
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

                            // LOC thresholds per file under src/*.rs
                            let src_dir = pdir.join("src");
                            if src_dir.exists() {
                                for ent in fs::read_dir(&src_dir)? {
                                    let ent = ent?;
                                    let p = ent.path();
                                    if p.extension() == Some(OsStr::new("rs")) {
                                        let cnt = std::fs::read_to_string(&p).map(|s| s.lines().count()).unwrap_or(0) as u64;
                                        if cnt > check.src_loc_err_over {
                                            check.files_over_err.push(p.display().to_string());
                                            check.errors.push(format!("oversized file: {} ({} LOC)", p.display(), cnt));
                                        } else if cnt > check.src_loc_warn_over {
                                            check.files_over_warn.push(p.display().to_string());
                                            check.warnings.push(format!("large file: {} ({} LOC)", p.display(), cnt));
                                        }
                                    }
                                }
                            }

                            // tests/ presence (at least one .rs)
                            let tests_dir = pdir.join("tests");
                            let has_rs = tests_dir.exists() && std::fs::read_dir(&tests_dir).ok()
                                .map(|it| it.filter_map(|e| e.ok()).any(|e| e.path().extension()==Some(OsStr::new("rs"))))
                                .unwrap_or(false);
                            check.has_tests_dir = has_rs;
                            if !check.has_tests_dir {
                                check.warnings.push("missing tests dir".into());
                            }

                            // README AUTO (id=commands) marker
                            if check.has_readme {
                                if let Ok(rtxt) = fs::read_to_string(&readme) {
                                    let has = rtxt.contains("<!-- BEGIN: AUTO (id=commands) -->");
                                    check.readme_has_commands_auto = has;
                                    if !has {
                                        check.warnings.push("README missing AUTO (id=commands)".into());
                                    }
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

                    if m.get_flag("strict") && result.not_ok > 0 {
                        let exit = ctx.err("dev","plugins.check","plugins.invalid","One or more plugins have structural errors", Some("Run locally: `nexoscli dev plugins check --fix-readmes`"), 2);
                        return Ok(exit);
                    }

                    // Soft-report result to keep CI green during migration; warnings/errors encoded in data
                    let exit = ctx.ok("dev","plugins.check", &result);
                    return Ok(exit);
                }
                Ok(2)
            }

            // ---------------- dev repo ----------------
            Some(("repo", rm)) => {
                match rm.subcommand() {
                    Some(("policy", pm)) => {
                        if let Some(("init", im)) = pm.subcommand() {
                            let root = std::env::current_dir()?;
                            let force = im.get_flag("force");
                            let path = crate::repo::write_default_policy(&root, force)?;
                            let exit = ctx.ok("dev","repo.policy.init", &serde_json::json!({ "written": path.display().to_string(), "force": force }));
                            return Ok(exit);
                        }
                        Ok(2)
                    }
                    Some(("check", cm)) => {
                        let root = std::env::current_dir()?;
                        let pol = cm.get_one::<String>("policy").map(|s| PathBuf::from(s));
                        let fix = cm.get_flag("fix-readmes");
                        let res = crate::repo::check_repo(&root, pol.as_deref(), fix)?;
                        if res.ok {
                            let exit = ctx.ok("dev","repo.check",&res);
                            return Ok(exit);
                        } else {
                            let exit = ctx.err("dev","repo.check","repo.structure.invalid","Repo violations found", Some("Run with --fix-readmes; or update policy via dev repo policy init"), 2);
                            return Ok(exit);
                        }
                    }
                    Some(("readmes", rm2)) => {
                        let root = std::env::current_dir()?;
                        let pol = rm2.get_one::<String>("policy").map(|s| PathBuf::from(s));
                        let apply = rm2.get_flag("apply");
                        let res = crate::repo::check_repo(&root, pol.as_deref(), apply)?;
                        let exit = ctx.ok("dev","repo.readmes",&res);
                        return Ok(exit);
                    }
                    _ => Ok(2),
                }
            }

            // ---------------- dev log ----------------
            Some(("log", m)) => {
                cmd_log::run(m, ctx)
            }

            // ---------------- dev doctor ----------------
            Some(("doctor", m)) => {
                if m.get_flag("ci") {
                    let cwd = std::env::current_dir()?;
                    let mut want: std::collections::HashSet<String> = m
                        .get_one::<String>("checks")
                        .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
                        .unwrap_or_else(|| ["graph","plugins","config","events","tools","cline"]
                            .into_iter().map(|s| s.to_string()).collect());
                    if m.get_one::<String>("profile").map(|s| s == "v0").unwrap_or(false) {
                        want = ["graph","plugins","config","events","tools","cline","rules"]
                            .into_iter().map(|s| s.to_string()).collect();
                    }

                    // optional time window for events-schema in CI (24h default from flag)
                    let since_ts: Option<OffsetDateTime> = m.get_one::<String>("since").and_then(|s| {
                        if s.ends_with("h") {
                            s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h))
                        } else if s.ends_with("d") {
                            s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d))
                        } else {
                            OffsetDateTime::parse(s, &Rfc3339).ok()
                        }
                    });

                    let mut checks = Vec::<DoctorCiCheck>::new();

                    // Run log integrity checks first to avoid mutations by subsequent checks
                    // events-chain (verify early; read-only)
                    if want.contains("events-chain") {
                        let outc = crate::checks::check_events_chain(&cwd, crate::checks::ChainMode::Verify);
                        let ok = outc.ok;
                        let detail = if ok {
                            format!("digest={} lines={} file={}", outc.digest.unwrap_or_default(), outc.lines, outc.file.unwrap_or_default())
                        } else {
                            format!("errors: {}", outc.errors.join("; "))
                        };
                        checks.push(DoctorCiCheck{
                            name: "events-chain".into(),
                            ok,
                            detail,
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: Some("Run locally: `nexoscli dev doctor --checks events-chain` to refresh chain file".into()),
                        });
                    }

                    // events-schema (validate early; read-only)
                    if want.contains("events-schema") {
                        let outc = crate::checks::check_events_schema(&cwd, since_ts);
                        let ok = outc.ok;
                        let detail = if ok {
                            format!("lines={} file={}", outc.lines, outc.file.unwrap_or_default())
                        } else {
                            format!("errors: {}", outc.errors.join("; "))
                        };
                        checks.push(DoctorCiCheck{
                            name: "events-schema".into(),
                            ok,
                            detail,
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: Some("Inspect .nexoscli/log/events.jsonl; ensure lines are valid event.v0.3".into()),
                        });
                    }

                    // events-rotation (index & segments continuity)
                    if want.contains("events-rotation") {
                        let outc = crate::checks::check_events_rotation(&cwd);
                        let ok = outc.ok;
                        let detail = if ok {
                            format!("segments={} head={}", outc.lines, outc.digest.unwrap_or_default())
                        } else {
                            format!("errors: {}", outc.errors.join("; "))
                        };
                        checks.push(DoctorCiCheck{
                            name: "events-rotation".into(),
                            ok,
                            detail,
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: Some("Run `nexoscli dev log rotate --max-bytes 5000000 --daily` locally; commit updated index if applicable".into()),
                        });
                    }

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

                    // events: try to write a ping (but skip if events-chain verify is requested to avoid mutating the log)
                    if want.contains("events") {
                        let p = std::path::PathBuf::from(".nexoscli/log/events.jsonl");
                        if want.contains("events-chain") {
                            // Skip write to keep chain stable for verification
                            checks.push(DoctorCiCheck {
                                name: "event_log_writable".into(),
                                ok: true,
                                detail: format!("skipped due to events-chain verify: {}", p.display()),
                                severity: "info".into(),
                                fix: None,
                            });
        } else {
            // Ensure directory exists in ephemeral test/CI contexts
            let pdir = std::path::PathBuf::from(".nexoscli/log");
            let _ = std::fs::create_dir_all(&pdir);
            let write_res = ctx.events.write("dev","doctor.ci.ping",&serde_json::json!({"ping":true}),0,None,None,None);
            let ok = write_res.is_ok();
            let detail = if ok {
                p.display().to_string()
            } else {
                format!("{} (write_err)", p.display())
            };
            checks.push(DoctorCiCheck {
                name: "event_log_writable".into(),
                ok,
                detail,
                severity: if ok { "info".into() } else { "error".into() },
                fix: if ok { None } else { Some("Ensure .nexoscli/log/ directory is writable".into()) },
            });
        }
                    }

                    // graph
                    if want.contains("graph") {
                        let graph_file = cwd.join("docs").join("exec_graph.yaml");
                        if !graph_file.exists() {
                            // Tolerate missing graph in ephemeral/temp repos; treat as OK (informational)
                            checks.push(DoctorCiCheck {
                                name: "graph_check".into(),
                                ok: true,
                                detail: format!("missing {}; skip", graph_file.display()),
                                severity: "info".into(),
                                fix: Some("Create docs/exec_graph.yaml to enable graph validation".into()),
                            });
                        } else {
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

                    // repo (optional)
                    if want.contains("repo") {
                        let res = crate::repo::check_repo(&cwd, None, false)?;
                        let ok = res.ok;
                        let mut detail = res.summary.clone();
                        if !ok {
                            let mut viols = Vec::new();
                            for it in &res.items {
                                if !it.ok {
                                    viols.push(format!("{}: {}", it.name, it.errors.get(0).cloned().unwrap_or_default()));
                                }
                            }
                            if !viols.is_empty() {
                                detail = format!("violations → {}", viols.join(" | "));
                            }
                        }
                        checks.push(DoctorCiCheck {
                            name: "repo".into(),
                            ok,
                            detail,
                            severity: if ok { "info".into() } else { "error".into() },
                            fix: Some("Run `nexoscli dev repo check --fix-readmes` and/or `nexoscli dev repo policy init`".into()),
                        });
                    }

                    // plans (plan lifecycle invariants)
                    if want.contains("plans") {
                        let res = verify_plans(&cwd, None)?;
                        let order = ["current","next","done","backlog","archived","paused","dumped"];
                        let mut parts: Vec<String> = Vec::new();
                        for name in order {
                            if let Some(it) = res.items.iter().find(|x| x.name == name) {
                                if let Some(c) = it.count { parts.push(format!("{}={}", name, c)); }
                            }
                        }
                        let mut detail = if parts.is_empty() { String::new() } else { format!("counts: {}", parts.join(" ")) };
                        if !res.ok {
                            let mut viols: Vec<String> = Vec::new();
                            for it in &res.items {
                                for e in &it.errors { viols.push(format!("{}: {}", it.name, e)); }
                            }
                            let head = viols.into_iter().take(3).collect::<Vec<_>>().join(" | ");
                            if detail.is_empty() { detail = format!("violations: {}", head); } else { detail = format!("{}; violations: {}", detail, head); }
                        }
                        checks.push(DoctorCiCheck {
                            name: "plans".into(),
                            ok: res.ok,
                            detail: if detail.is_empty() { "plans OK".into() } else { detail },
                            severity: if res.ok { "info".into() } else { "error".into() },
                            fix: Some("Run `nexoscli dev plans verify` and fix invariants; refresh indexes with `nexoscli dev plans index build`".into()),
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
                                    // correlation coverage for last 24h (dry-run)
                                    let (_c_cor, v_cor) = run_cli_json(&cwd, &["cline","correlate","--since","24h","--pad","5m","--dry-run"])?;
                                    let cov = v_cor["data"]["summary"]["coverage"].as_f64().unwrap_or(0.0);
                                    let total = v_cor["data"]["summary"]["runs_total"].as_u64().unwrap_or(0);
                                    let msg_cov = v_cor["data"]["message_coverage"].as_f64().unwrap_or(0.0);
                                    detail = format!("storage={} imported~{} task_cov={:.0}% msg_cov={:.0}% over {} runs", storage, imp, cov*100.0, msg_cov*100.0, total);
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
                    if m.get_one::<String>("profile").map(|s| s == "v0").unwrap_or(false) {
                        required.insert("event_log_writable".into());
                    }
                    // If --checks explicitly provided and non-empty, treat selected as required
                    let checks_explicit = m.value_source("checks") == Some(ValueSource::CommandLine)
                        && m.get_one::<String>("checks").map(|s| !s.trim().is_empty()).unwrap_or(false);
                    if checks_explicit {
                        for k in &want { required.insert(k.clone()); }
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

                    // events-schema (always computed locally; honors --since window)
                    let since_ts: Option<OffsetDateTime> = {
                        let s = since;
                        if s.ends_with('h') {
                            s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h))
                        } else if s.ends_with('d') {
                            s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d))
                        } else {
                            OffsetDateTime::parse(s, &Rfc3339).ok()
                        }
                    };
                    let root = std::env::current_dir()?;
                    let es = crate::checks::check_events_schema(&root, since_ts);
                    report.insert("events_schema".into(), serde_json::json!({
                        "ok": es.ok, "lines": es.lines, "errors": es.errors, "file": es.file
                    }));
                    if !es.ok && require.iter().any(|c| *c == "events-schema") { hard_fail = true; }

                    // events-chain (local refresh mode writes/updates chain file)
                    let ec = crate::checks::check_events_chain(&root, crate::checks::ChainMode::Refresh);
                    report.insert("events_chain".into(), serde_json::json!({
                        "ok": ec.ok, "lines": ec.lines, "digest": ec.digest, "errors": ec.errors, "file": ec.file
                    }));
                    if !ec.ok && require.iter().any(|c| *c == "events-chain") { hard_fail = true; }

                    // events-rotation (local report; verify continuity)
                    let rot = crate::checks::check_events_rotation(&root);
                    report.insert("events_rotation".into(), serde_json::json!({
                        "ok": rot.ok, "segments": rot.lines, "head": rot.digest, "errors": rot.errors, "file": rot.file
                    }));
                    if !rot.ok && require.iter().any(|c| *c == "events-rotation") { hard_fail = true; }

                    // Add overall ready status for basic usage
                    report.insert("ready".into(), serde_json::json!(!hard_fail));

                    let code = if hard_fail { ctx.err("dev","doctor","DOCTOR_FAILED","one or more required checks failed",None,2) } else { ctx.ok("dev","doctor",&report) };
                    // Post-emit refresh so the chain file includes this doctor event line as well
                    let _ = crate::checks::check_events_chain(&root, crate::checks::ChainMode::Refresh);
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
                } else if let Some(("hai", m)) = tm.subcommand() {
                    let since_s = m.get_one::<String>("since").cloned();
                    let since_ts: Option<OffsetDateTime> = match since_s.as_deref() {
                        None => None,
                        Some(s) if s.ends_with('h') => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
                        Some(s) if s.ends_with('d') => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
                        Some(s) => OffsetDateTime::parse(s, &Rfc3339).ok(),
                    };

                    let root = std::env::current_dir()?;
                    let streams = root.join(".nexoscli").join("agents").join("streams");
                    let mut prompts: u64 = 0;
                    let mut responses: u64 = 0;
                    let mut first_reply_latencies: Vec<u64> = Vec::new();
                    let mut code_blocks: u64 = 0;

                    if streams.exists() {
                        for prov in std::fs::read_dir(&streams)? {
                            let prov = prov?;
                            if !prov.file_type()?.is_dir() { continue; }
                            for sid in std::fs::read_dir(prov.path())? {
                                let sid = sid?;
                                if !sid.file_type()?.is_dir() { continue; }
                                let msgs = sid.path().join("messages.ndjson");
                                if !msgs.exists() { continue; }
                                let s = std::fs::read_to_string(&msgs)?;
                                let mut last_user_ts: Option<OffsetDateTime> = None;
                                let mut counted_reply = false;
                                for line in s.lines() {
                                    let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, _ => continue };
                                    let ts = v.get("created_at").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
                                    if let Some(since) = since_ts {
                                        if ts.map(|t| t < since).unwrap_or(false) { continue; }
                                    }
                                    let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("assistant");
                                    match role {
                                        "user" => { prompts += 1; last_user_ts = ts; counted_reply = false; }
                                        "assistant" => {
                                            responses += 1;
                                            if let (Some(u), Some(a)) = (last_user_ts, ts) {
                                                if !counted_reply && a >= u {
                                                    first_reply_latencies.push((a - u).whole_milliseconds().max(0) as u64);
                                                    counted_reply = true;
                                                }
                                            }
                                            if let Some(arr) = v.get("code_blocks").and_then(|x| x.as_array()) {
                                                code_blocks += arr.len() as u64;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }

                    first_reply_latencies.sort_unstable();
                    let p50 = percentile(&first_reply_latencies, 50.0);
                    let p90 = percentile(&first_reply_latencies, 90.0);
                    let out = serde_json::json!({
                        "since": since_s,
                        "prompts": prompts,
                        "responses": responses,
                        "reply_latency_ms": { "p50": p50, "p90": p90, "count": first_reply_latencies.len() },
                        "code_blocks": code_blocks
                    });
                    let exit = ctx.ok("dev","telemetry.hai",&out);
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
                } else if let Some(("commands", m)) = dm.subcommand() {
                    return crate::cmd_docs::run(m, ctx);
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

            // ---------------- dev plans ----------------
            Some(("plans", pm)) => {
                match pm.subcommand() {
                    Some(("list", m)) => {
                        let which = m.get_one::<String>("where").cloned().unwrap_or_else(|| "all".to_string());
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let items = list_plans(&root, &which)?;
                        let out = PlanListResult{ total: items.len(), items };
                        let exit = ctx.ok("dev","plans.list",&out);
                        Ok(exit)
                    }
                    Some(("next", _m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let p = pick_next_plan(&root).ok_or_else(|| anyhow::anyhow!("no plan under docs/project/next"))?;
                        let meta = read_plan_meta(&p)?;
                        let (plan_id, slug) = derive_id_slug(&p);
                        let out = serde_json::json!({
                            "plan": p.to_string_lossy(),
                            "plan_id": plan_id,
                            "slug": slug,
                            "title": meta.title,
                            "status": meta.status
                        });
                        let exit = ctx.ok("dev","plans.next",&out);
                        Ok(exit)
                    }
                    Some(("status", m)) => {
                        let plan = PathBuf::from(m.get_one::<String>("plan").unwrap());
                        let meta = read_plan_meta(&plan)?;
                        let (plan_id, slug) = derive_id_slug(&plan);
                        let out = PlanStatusResult {
                            plan: plan.display().to_string(),
                            frontmatter: meta.front_json(),
                            title: meta.title.clone(),
                            plan_id,
                            slug,
                            status: meta.status.clone(),
                        };
                        let exit = ctx.ok("dev","plans.status",&out);
                        Ok(exit)
                    }
                    Some(("start", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan_opt = m.get_one::<String>("plan").cloned();
                        let dry = m.get_flag("dry-run");
                        let plan_path = match plan_opt {
                            Some(p) => PathBuf::from(p),
                            None => pick_next_plan(&root).ok_or_else(|| anyhow::anyhow!("no plan under docs/project/next"))?,
                        };
                        let (plan_id, slug) = derive_id_slug(&plan_path);
                        let meta0 = read_plan_meta(&plan_path)?;
                        if let Some(s) = meta0.status.as_deref() {
                            if s.eq_ignore_ascii_case("in-progress") || s.eq_ignore_ascii_case("done") {
                                let exit = ctx.err(
                                    "dev",
                                    "plans.start",
                                    "plan.status.invalid",
                                    &format!("plan status is '{}', cannot start", s),
                                    None,
                                    2
                                );
                                return Ok(exit);
                            }
                        }
                        let title = meta0.title.clone().unwrap_or_else(|| slug.clone().unwrap_or_else(|| "plan".into()));
                        let (c_new, v_new) = run_cli_json(&root, &["task","new","--title", &title])?;
                        if c_new != 0 {
                            let exit = ctx.err("dev","plans.start","task.new.failed","task new failed", None, 2);
                            return Ok(exit);
                        }
                        let task_id = v_new["data"]["task_id"].as_str()
                            .or_else(|| v_new["data"]["id"].as_str())
                            .or_else(|| v_new["meta"]["context"]["task_id"].as_str())
                            .unwrap_or("").to_string();
                        if task_id.is_empty() {
                            let exit = ctx.err("dev","plans.start","task.id.missing","could not extract task_id", None, 2);
                            return Ok(exit);
                        }
                        let (c_start, _v_start) = run_cli_json(&root, &["task","start","--task",&task_id])?;
                        if c_start != 0 {
                            let exit = ctx.err("dev","plans.start","task.start.failed","task start failed", None, 2);
                            return Ok(exit);
                        }

                        let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
                        let mut meta = meta0.clone();
                        meta.status = Some("in-progress".into());
                        meta.plan_id = plan_id.clone();
                        meta.slug = slug.clone();
                        meta.task_id = Some(task_id.clone());
                        meta.started_at = Some(now);
                        meta.source = Some("next".into());

                        if dry {
                            let out = PlanStartResult {
                                picked_plan: plan_path.display().to_string(),
                                moved_from: plan_path.display().to_string(),
                                moved_to: format!("docs/project/current/{}", plan_path.file_name().unwrap().to_string_lossy()),
                                task_id, title, plan_id: plan_id.unwrap_or_default(), slug: slug.unwrap_or_default(),
                                updated_frontmatter: meta.front_json(),
                            };
                            let exit = ctx.ok("dev","plans.start.dryrun",&out);
                            return Ok(exit);
                        }

                        write_plan_meta(&plan_path, &meta)?;
                        let dst = root.join("docs").join("project").join("current")
                            .join(plan_path.file_name().unwrap());
                        std::fs::rename(&plan_path, &dst)?;

                        let out = PlanStartResult {
                            picked_plan: dst.display().to_string(),
                            moved_from: format!("docs/project/next/{}", dst.file_name().unwrap().to_string_lossy()),
                            moved_to: dst.display().to_string(),
                            task_id, title, plan_id: plan_id.unwrap_or_default(), slug: slug.unwrap_or_default(),
                            updated_frontmatter: meta.front_json(),
                        };
                        let exit = ctx.ok("dev","plans.start",&out);
                        Ok(exit)
                    }
                    Some(("finalize", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan_opt = m.get_one::<String>("plan").cloned();
                        let plan_path = match plan_opt {
                            Some(p) => PathBuf::from(p),
                            None => pick_single_current(&root)?
                        };
                        let mut meta = read_plan_meta(&plan_path)?;
                        let task_id = meta.task_id.clone();

                        let impl_in = if let Some(x) = m.get_one::<String>("implemented-in") {
                            x.clone()
                        } else {
                            git_rev_parse_head().unwrap_or_default()
                        };
                        let mut bundle_out = None;
                        if let Some(tid) = &task_id {
                            let _ = run_cli_json(&root, &["task","end"]);
                            let out_path = if let Some(path) = m.get_one::<String>("bundle-out") {
                                path.clone()
                            } else {
                                format!("{}-bundle.tgz", tid)
                            };
                            let (_c_b, _v_b) = run_cli_json(&root, &["task","bundle","--task",tid,"--out",&out_path])?;
                            bundle_out = Some(out_path);
                        }

                        let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
                        meta.status = Some("done".into());
                        if !impl_in.is_empty() { meta.implemented_in = Some(impl_in.clone()); }
                        meta.completed_at = Some(now);

                        // Dry-run path: report intended move and metadata without applying changes
                        let dry = m.get_flag("dry-run");
                        if dry {
                            let dst = root.join("docs").join("project").join("done")
                                .join(plan_path.file_name().unwrap());
                            // Ensure implemented_in key is present in preview even when git HEAD is unavailable
                            if meta.implemented_in.is_none() { meta.implemented_in = Some(impl_in.clone()); }
                            let out = PlanFinalizeResult {
                                plan: dst.display().to_string(),
                                moved_to: dst.display().to_string(),
                                task_id,
                                implemented_in: impl_in.clone(),
                                bundle_out,
                                frontmatter: meta.front_json(),
                            };
                            let exit = ctx.ok("dev","plans.finalize.dryrun",&out);
                            return Ok(exit);
                        }

                        write_plan_meta(&plan_path, &meta)?;
                        let dst = root.join("docs").join("project").join("done")
                            .join(plan_path.file_name().unwrap());
                        std::fs::rename(&plan_path, &dst)?;

                        let out = PlanFinalizeResult {
                            plan: dst.display().to_string(),
                            moved_to: dst.display().to_string(),
                            task_id,
                            implemented_in: impl_in,
                            bundle_out,
                            frontmatter: meta.front_json(),
                        };
                        let exit = ctx.ok("dev","plans.finalize",&out);
                        Ok(exit)
                    }
                    Some(("pause", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan_opt = m.get_one::<String>("plan").cloned();
                        let reason = m.get_one::<String>("reason").cloned();
                        let plan_path = match plan_opt {
                            Some(p) => PathBuf::from(p),
                            None => pick_single_current(&root)?,
                        };
                        let mut meta = read_plan_meta(&plan_path)?;
                        if let Some(r) = &reason {
                            meta.front.insert(serde_yaml::Value::String("rationale".into()), serde_yaml::Value::String(r.clone()));
                        }
                        let dry = m.get_flag("dry-run");
                        let dst = root.join("docs").join("project").join("paused")
                            .join(plan_path.file_name().unwrap());
                        if dry {
                            let out = PlanPauseResult {
                                plan: dst.display().to_string(),
                                moved_to: dst.display().to_string(),
                                rationale: reason.clone(),
                                frontmatter: meta.front_json(),
                            };
                            let exit = ctx.ok("dev","plans.pause.dryrun",&out);
                            return Ok(exit);
                        }
                        write_plan_meta(&plan_path, &meta)?;
                        std::fs::rename(&plan_path, &dst)?;
                        let out = PlanPauseResult {
                            plan: dst.display().to_string(),
                            moved_to: dst.display().to_string(),
                            rationale: reason,
                            frontmatter: meta.front_json(),
                        };
                        let exit = ctx.ok("dev","plans.pause",&out);
                        Ok(exit)
                    }
                    Some(("resume", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        // current must be empty
                        let cur_dir = root.join("docs").join("project").join("current");
                        let cur_count = std::fs::read_dir(&cur_dir).ok()
                            .map(|it| it.filter_map(|e| e.ok())
                                .filter(|e| e.path().extension()==Some(OsStr::new("md")))
                                .filter(|e| e.file_name().to_string_lossy().to_lowercase().starts_with("plan-"))
                                .count()).unwrap_or(0);
                        if cur_count > 0 {
                            let exit = ctx.err("dev","plans.resume","plans.current.nonempty","current/ not empty; cannot resume", Some("Finalize or pause current plan first"), 2);
                            return Ok(exit);
                        }
                        let plan_opt = m.get_one::<String>("plan").cloned();
                        let plan_path = match plan_opt {
                            Some(p) => PathBuf::from(p),
                            None => pick_single_paused(&root)?,
                        };
                        // reconciliation: verify + repo checks
                        let plans = verify_plans(&root, None)?;
                        let repo = crate::repo::check_repo(&root, None, false)?;
                        let recon = serde_json::json!({
                            "plans_ok": plans.ok,
                            "plans_summary": plans.summary,
                            "repo_ok": repo.ok,
                            "repo_summary": repo.summary,
                        });

                        let dry = m.get_flag("dry-run");
                        let dst = root.join("docs").join("project").join("current")
                            .join(plan_path.file_name().unwrap());
                        if dry {
                            let out = PlanResumeResult {
                                plan: dst.display().to_string(),
                                moved_to: dst.display().to_string(),
                                current_was_empty: true,
                                reconciliation: recon,
                            };
                            let exit = ctx.ok("dev","plans.resume.dryrun",&out);
                            return Ok(exit);
                        }
                        std::fs::rename(&plan_path, &dst)?;
                        let out = PlanResumeResult {
                            plan: dst.display().to_string(),
                            moved_to: dst.display().to_string(),
                            current_was_empty: true,
                            reconciliation: recon,
                        };
                        let exit = ctx.ok("dev","plans.resume",&out);
                        Ok(exit)
                    }
                    Some(("archive", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan = PathBuf::from(m.get_one::<String>("plan").unwrap());
                        let reason = m.get_one::<String>("reason").cloned();
                        let mut meta = read_plan_meta(&plan)?;
                        if let Some(r) = &reason {
                            meta.front.insert(serde_yaml::Value::String("rationale".into()), serde_yaml::Value::String(r.clone()));
                        }
                        let dry = m.get_flag("dry-run");
                        let dst = root.join("docs").join("project").join("archived")
                            .join(plan.file_name().unwrap());
                        if dry {
                            let out = PlanArchiveResult {
                                plan: dst.display().to_string(),
                                moved_to: dst.display().to_string(),
                                rationale: reason.clone(),
                                frontmatter: meta.front_json(),
                            };
                            let exit = ctx.ok("dev","plans.archive.dryrun",&out);
                            return Ok(exit);
                        }
                        write_plan_meta(&plan, &meta)?;
                        std::fs::rename(&plan, &dst)?;
                        let out = PlanArchiveResult {
                            plan: dst.display().to_string(),
                            moved_to: dst.display().to_string(),
                            rationale: reason,
                            frontmatter: meta.front_json(),
                        };
                        let exit = ctx.ok("dev","plans.archive",&out);
                        Ok(exit)
                    }
                    Some(("dump", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan = PathBuf::from(m.get_one::<String>("plan").unwrap());
                        let reason = m.get_one::<String>("reason").cloned();
                        let mut meta = read_plan_meta(&plan)?;
                        if let Some(r) = &reason {
                            meta.front.insert(serde_yaml::Value::String("rationale".into()), serde_yaml::Value::String(r.clone()));
                        }
                        let dry = m.get_flag("dry-run");
                        let dst = root.join("docs").join("project").join("dumped")
                            .join(plan.file_name().unwrap());
                        if dry {
                            let out = PlanDumpResult {
                                plan: dst.display().to_string(),
                                moved_to: dst.display().to_string(),
                                rationale: reason.clone(),
                                frontmatter: meta.front_json(),
                            };
                            let exit = ctx.ok("dev","plans.dump.dryrun",&out);
                            return Ok(exit);
                        }
                        write_plan_meta(&plan, &meta)?;
                        std::fs::rename(&plan, &dst)?;
                        let out = PlanDumpResult {
                            plan: dst.display().to_string(),
                            moved_to: dst.display().to_string(),
                            rationale: reason,
                            frontmatter: meta.front_json(),
                        };
                        let exit = ctx.ok("dev","plans.dump",&out);
                        Ok(exit)
                    }
                    Some(("requeue", m)) => {
                        let root = std::env::current_dir()?;
                        ensure_project_plan_dirs(&root)?;
                        let plan_opt = m.get_one::<String>("plan").cloned();
                        let plan = match plan_opt {
                            Some(p) => PathBuf::from(p),
                            None => pick_single_paused(&root)?,
                        };
                        let fname = plan.file_name().unwrap().to_string_lossy().to_string();
                        let mut warnings = Vec::new();
                        if fname.starts_with("PLAN-") {
                            warnings.push("numbered plan in backlog not recommended".to_string());
                        }
                        let dry = m.get_flag("dry-run");
                        let dst = root.join("docs").join("project").join("backlog")
                            .join(plan.file_name().unwrap());
                        if dry {
                            let out = PlanRequeueResult {
                                plan: dst.display().to_string(),
                                moved_to: dst.display().to_string(),
                                warnings,
                            };
                            let exit = ctx.ok("dev","plans.requeue.dryrun",&out);
                            return Ok(exit);
                        }
                        std::fs::rename(&plan, &dst)?;
                        let out = PlanRequeueResult {
                            plan: dst.display().to_string(),
                            moved_to: dst.display().to_string(),
                            warnings,
                        };
                        let exit = ctx.ok("dev","plans.requeue",&out);
                        Ok(exit)
                    }
                    Some(("verify", m)) => {
                        let root = std::env::current_dir()?;
                        let base = m.get_one::<String>("path").map(|s| PathBuf::from(s));
                        let res = verify_plans(&root, base.as_deref())?;
                        let exit = ctx.ok("dev","plans.verify",&res);
                        Ok(exit)
                    }
                    Some(("index", im)) => {
                        if let Some(("build", bm)) = im.subcommand() {
                            let root = std::env::current_dir()?;
                            let base = bm.get_one::<String>("path").map(|s| PathBuf::from(s));
                            let dry = bm.get_flag("dry-run");
                            let res = index_build(&root, base.as_deref(), dry)?;
                            let exit = ctx.ok("dev","plans.index.build",&res);
                            Ok(exit)
                        } else {
                            Ok(2)
                        }
                    }
                    _ => Ok(2)
                }
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
                args_schema: schema_for!(cmd_init::DevInitArgs),
                result_schema: schema_for!(cmd_init::DevInitResult),
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
            // dev.plans.start
            ToolSpec {
                name: "dev.plans.start".to_string(),
                about: "Start a plan: create+start task, update front-matter, move next→current".to_string(),
                args_schema: schema_for!(PlanStartArgs),
                result_schema: schema_for!(PlanStartResult),
            },
            // dev.plans.finalize
            ToolSpec {
                name: "dev.plans.finalize".to_string(),
                about: "Finalize a plan: mark done, end task, bundle, move current→done".to_string(),
                args_schema: schema_for!(PlanFinalizeArgs),
                result_schema: schema_for!(PlanFinalizeResult),
            },
            // dev.plans.pause
            ToolSpec {
                name: "dev.plans.pause".to_string(),
                about: "Pause current plan: move current→paused".to_string(),
                args_schema: schema_for!(PlanPauseArgs),
                result_schema: schema_for!(PlanPauseResult),
            },
            // dev.plans.resume
            ToolSpec {
                name: "dev.plans.resume".to_string(),
                about: "Resume a paused plan: move paused→current".to_string(),
                args_schema: schema_for!(PlanResumeArgs),
                result_schema: schema_for!(PlanResumeResult),
            },
            // dev.plans.archive
            ToolSpec {
                name: "dev.plans.archive".to_string(),
                about: "Archive a plan: move to archived/".to_string(),
                args_schema: schema_for!(PlanArchiveArgs),
                result_schema: schema_for!(PlanArchiveResult),
            },
            // dev.plans.dump
            ToolSpec {
                name: "dev.plans.dump".to_string(),
                about: "Dump a plan: move to dumped/".to_string(),
                args_schema: schema_for!(PlanDumpArgs),
                result_schema: schema_for!(PlanDumpResult),
            },
            // dev.plans.requeue
            ToolSpec {
                name: "dev.plans.requeue".to_string(),
                about: "Requeue a paused plan to backlog/".to_string(),
                args_schema: schema_for!(PlanRequeueArgs),
                result_schema: schema_for!(PlanRequeueResult),
            },
            // dev.plans.list
            ToolSpec {
                name: "dev.plans.list".to_string(),
                about: "List plans in docs/project/{next,current,done,archived}".to_string(),
                args_schema: schema_for!(PlanListArgs),
                result_schema: schema_for!(PlanListResult),
            },
            // dev.plans.status
            ToolSpec {
                name: "dev.plans.status".to_string(),
                about: "Show plan front-matter and derived ids".to_string(),
                args_schema: schema_for!(PlanStatusArgs),
                result_schema: schema_for!(PlanStatusResult),
            },
            // dev.plans.next
            ToolSpec {
                name: "dev.plans.next".to_string(),
                about: "Show the next plan candidate (docs/project/next)".to_string(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
            },
            // dev.plans.verify
            ToolSpec {
                name: "dev.plans.verify".to_string(),
                about: "Validate plan lifecycle folder rules".to_string(),
                args_schema: schema_for!(PlansVerifyArgs),
                result_schema: schema_for!(PlansVerifyResult),
            },
            // dev.plans.index.build
            ToolSpec {
                name: "dev.plans.index.build".to_string(),
                about: "Refresh AUTO (id=plans-index) blocks in docs/project/* READMEs".to_string(),
                args_schema: schema_for!(PlansIndexBuildArgs),
                result_schema: schema_for!(PlansIndexResult),
            },
            // repo checks
            ToolSpec {
                name: "dev.repo.check".to_string(),
                about: "Validate repo structure/entropy against policy".to_string(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
            },
            ToolSpec {
                name: "dev.repo.policy.init".to_string(),
                about: "Write default .nexoscli/repo_policy.yaml".to_string(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
            },
            ToolSpec {
                name: "dev.repo.readmes".to_string(),
                about: "Scaffold missing folder READMEs".to_string(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
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

pub(crate) fn needs_gitignore_line(path: &PathBuf, line: &str) -> std::io::Result<bool> {
    if !path.exists() { return Ok(true); }
    let content = std::fs::read_to_string(path)?;
    Ok(!content.lines().any(|l| l.trim() == line.trim()))
}
pub(crate) fn append_gitignore_line(path: &PathBuf, line: &str) -> std::io::Result<()> {
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    if needs_gitignore_line(path, line)? {
        if f.metadata()?.len() > 0 { write!(f, "\n")?; }
        writeln!(f, "{}", line)?;
    }
    Ok(())
}

/// Collect all regular files under a template directory (recursive)
fn collect_template_files(base: &Path) -> anyhow::Result<Vec<PathBuf>> {
    fn walk(dir: &Path, acc: &mut Vec<PathBuf>) -> anyhow::Result<()> {
        for ent in std::fs::read_dir(dir)? {
            let ent = ent?;
            let p = ent.path();
            if ent.file_type()?.is_dir() {
                walk(&p, acc)?;
            } else if ent.file_type()?.is_file() {
                acc.push(p);
            }
        }
        Ok(())
    }
    let mut out = Vec::<PathBuf>::new();
    walk(base, &mut out)?;
    Ok(out)
}

/// Naive token substitution: replaces all occurrences of {{key}} with value
fn substitute_tokens(content: &str, tokens: &std::collections::HashMap<&str, String>) -> String {
    let mut out = content.to_string();
    for (k, v) in tokens {
        let pat = format!("{{{{{}}}}}", k);
        out = out.replace(&pat, v);
    }
    out
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

<!-- BEGIN: AUTO (id=commands) -->
_Generated by `nexoscli dev docs build` (PLAN‑54)._
<!-- END: AUTO -->

## Tool specs (for agents)

<!-- BEGIN: AUTO (id=tools) -->
_Generated by `nexoscli tools export` (if applicable)._
<!-- END: AUTO -->

## Implementation Notes

- This plugin implements `Plugin` with `name() = "{name}"`.
- Add/rename commands in `augment_commands()` and route in `execute()`.
- Keep outputs using the core envelope (`ctx.ok / ctx.err`) to stay machine-friendly.

"#)
}

// ===== Phase 7 helpers =====

#[derive(Clone)]
struct PlanMeta {
    front: serde_yaml::Mapping,
    title: Option<String>,
    status: Option<String>,
    plan_id: Option<String>,
    slug: Option<String>,
    task_id: Option<String>,
    started_at: Option<String>,
    completed_at: Option<String>,
    implemented_in: Option<String>,
    source: Option<String>,
}
impl PlanMeta {
    fn front_json(&self) -> serde_json::Value {
        // Merge typed fields into front-matter for an up-to-date preview,
        // so dry-run envelopes reflect intended mutations.
        let mut m = self.front.clone();
        let mut set = |k: &str, v: &Option<String>| {
            if let Some(val) = v.clone() {
                m.insert(
                    serde_yaml::Value::String(k.into()),
                    serde_yaml::Value::String(val),
                );
            }
        };
        set("status", &self.status);
        set("plan_id", &self.plan_id);
        set("slug", &self.slug);
        set("task_id", &self.task_id);
        set("started_at", &self.started_at);
        set("completed_at", &self.completed_at);
        set("implemented_in", &self.implemented_in);
        set("source", &self.source);

        serde_json::to_value(&m).unwrap_or(serde_json::json!({}))
    }
}

fn ensure_project_plan_dirs(root: &Path) -> anyhow::Result<()> {
    for d in [
        "docs/project/next",
        "docs/project/current",
        "docs/project/done",
        "docs/project/archived",
        "docs/project/backlog",
        "docs/project/paused",
        "docs/project/dumped",
    ] {
        let p = root.join(d);
        std::fs::create_dir_all(&p)?;
    }
    Ok(())
}

fn list_plans(root: &Path, which: &str) -> anyhow::Result<Vec<PlanRow>> {
    use std::fs;
    let mut out = vec![];
    let folders: Vec<&str> = match which {
        "next" => vec!["docs/project/next"],
        "current" => vec!["docs/project/current"],
        "done" => vec!["docs/project/done"],
        "archived" => vec!["docs/project/archived"],
        _ => vec!["docs/project/next","docs/project/current","docs/project/done","docs/project/archived"],
    };
    for f in folders {
        let dir = root.join(f);
        if !dir.exists() { continue; }
        for e in fs::read_dir(&dir)? {
            let e = e?;
            if !e.file_type()?.is_file() { continue; }
            if e.path().extension() != Some(OsStr::new("md")) { continue; }
            let file = e.file_name().to_string_lossy().to_string();
            if !file.to_lowercase().starts_with("plan-") { continue; }
            let meta = read_plan_meta(&e.path()).ok();
            let (pid, slug) = derive_id_slug(&e.path());
            out.push(PlanRow{
                folder: f.to_string(),
                file,
                plan_id: pid,
                slug,
                title: meta.as_ref().and_then(|m| m.title.clone()),
                status: meta.and_then(|m| m.status),
            });
        }
    }
    Ok(out)
}

fn pick_next_plan(root: &Path) -> Option<PathBuf> {
    let dir = root.join("docs/project/next");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension()==Some(OsStr::new("md")))
        .filter(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase().starts_with("plan-")).unwrap_or(false))
        .collect();
    files.sort();
    files.into_iter().next()
}

fn pick_single_current(root: &Path) -> anyhow::Result<PathBuf> {
    let dir = root.join("docs/project/current");
    let files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension()==Some(OsStr::new("md")))
        .filter(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase().starts_with("plan-")).unwrap_or(false))
        .collect();
    match files.len() {
        0 => anyhow::bail!("no plan under docs/project/current"),
        1 => Ok(files[0].clone()),
        _ => anyhow::bail!("multiple plans under docs/project/current; pass --plan"),
    }
}

fn pick_single_paused(root: &Path) -> anyhow::Result<PathBuf> {
    let dir = root.join("docs/project/paused");
    let files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|_| fs::read_dir(&dir).unwrap()) // will error anyway if missing
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension()==Some(OsStr::new("md")))
        .filter(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase().starts_with("plan-")).unwrap_or(false))
        .collect();
    match files.len() {
        0 => anyhow::bail!("no plan under docs/project/paused"),
        1 => Ok(files[0].clone()),
        _ => anyhow::bail!("multiple plans under docs/project/paused; pass --plan"),
    }
}

fn derive_id_slug(p: &Path) -> (Option<String>, Option<String>) {
    let base = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let s = base.trim_end_matches(".md");
    let s = if let Some(rest) = s.strip_prefix("PLAN-").or_else(|| s.strip_prefix("plan-")) { rest } else { return (None,None); };
    let mut id = String::new();
    let mut slug = String::new();
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_digit() {
            id.push(ch);
            continue;
        }
        let rest = &s[i+1..];
        slug = rest.trim_start_matches('_').trim_start_matches('-').to_string();
        break;
    }
    if id.is_empty() { return (None,None); }
    if slug.is_empty() {
        if let Some(pos) = s.find('_').or_else(|| s.find('-')) {
            slug = s[pos+1..].to_string();
        }
    }
    (Some(id), Some(slug))
}

fn read_plan_meta(p: &Path) -> anyhow::Result<PlanMeta> {
    let txt = std::fs::read_to_string(p)?;
    let mut front = serde_yaml::Mapping::new();
    let mut body = txt.as_str();
    let mut title = None;

    if txt.starts_with("---") {
        if let Some(idx) = txt[3..].find("\n---") {
            let head = &txt[3..3+idx];
            let rest = &txt[3+idx+4..];
            body = rest;
            if let Ok(v) = serde_yaml::from_str::<serde_yaml::Mapping>(head) {
                front = v;
            }
        }
    }
    for line in body.lines() {
        let l = line.trim_start();
        if let Some(t) = l.strip_prefix("# ") {
            title = Some(t.trim().to_string());
            break;
        }
    }
    let get = |k: &str| front.get(&serde_yaml::Value::String(k.into()))
        .and_then(|v| match v {
            serde_yaml::Value::String(s) => Some(s.clone()),
            _ => None
        });
    Ok(PlanMeta{
        front: front.clone(),
        title,
        status: get("status"),
        plan_id: get("plan_id"),
        slug: get("slug"),
        task_id: get("task_id"),
        started_at: get("started_at"),
        completed_at: get("completed_at"),
        implemented_in: get("implemented_in"),
        source: get("source"),
    })
}

fn write_plan_meta(p: &Path, meta: &PlanMeta) -> anyhow::Result<()> {
    let txt = std::fs::read_to_string(p)?;
    let mut front = meta.front.clone();
    let set = |m: &mut serde_yaml::Mapping, k: &str, v: Option<String>| {
        if let Some(val) = v {
            m.insert(serde_yaml::Value::String(k.into()), serde_yaml::Value::String(val));
        }
    };
    set(&mut front, "status", meta.status.clone());
    set(&mut front, "plan_id", meta.plan_id.clone());
    set(&mut front, "slug", meta.slug.clone());
    set(&mut front, "task_id", meta.task_id.clone());
    set(&mut front, "started_at", meta.started_at.clone());
    set(&mut front, "completed_at", meta.completed_at.clone());
    set(&mut front, "implemented_in", meta.implemented_in.clone());
    set(&mut front, "source", meta.source.clone());

    let body = if txt.starts_with("---") {
        if let Some(idx) = txt[3..].find("\n---") {
            &txt[3+idx+4..]
        } else { &txt }
    } else { &txt };

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&serde_yaml::to_string(&front)?);
    out.push_str("---\n");
    out.push_str(body.trim_start());
    std::fs::write(p, out.as_bytes())?;
    Ok(())
}

fn git_rev_parse_head() -> Option<String> {
    use std::process::Command;
    let out = Command::new("git").args(["rev-parse","--short","HEAD"]).output().ok()?;
    if !out.status.success() { return None; }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

//// ===== rotation helpers =====
use sha2::{Digest, Sha256};
use std::io::BufRead;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct IndexSeg {
    pub file: String,
    #[serde(skip_serializing_if="Option::is_none")] pub from_ts: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] pub to_ts: Option<String>,
    pub lines: u64,
    pub bytes: u64,
    pub sha256_file: String,
    #[serde(skip_serializing_if="Option::is_none")] pub prev_head: Option<String>,
    pub head: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub(crate) struct IndexFile {
    pub schema: String,                      // "events_index.v0.1"
    pub segments: Vec<IndexSeg>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub head: Option<String>,                // segment-chain head
    #[serde(skip_serializing_if="Option::is_none")]
    pub updated_at: Option<String>,
}

pub(crate) fn events_dir_from_log(log_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let ev = log_dir.join("events.jsonl");
    let seg_dir = log_dir.join("segments");
    let idx = log_dir.join("events.index.json");
    (ev, seg_dir, idx)
}

#[derive(Debug)]
struct ComputeStats {
    lines: u64,
    bytes: u64,
    from_ts: Option<String>,
    to_ts: Option<String>,
    sha256_file: String,
}

pub(crate) fn compute_event_file_stats(ev: &Path) -> anyhow::Result<ComputeStats> {
    let mut lines: u64 = 0;
    let mut bytes: u64 = 0;
    let mut h = Sha256::new();
    let mut from_ts: Option<String> = None;
    let mut to_ts: Option<String> = None;

    if ev.exists() {
        let f = fs::File::open(ev)?;
        bytes = f.metadata().map(|m| m.len()).unwrap_or(0);
        let rdr = std::io::BufReader::new(f);
        let mut first = true;
        for line in rdr.lines() {
            let raw = line?;
            if raw.trim().is_empty() { continue; }
            if !first { h.update(&[0x0a]); }
            h.update(raw.as_bytes());
            first = false;

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if from_ts.is_none() {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { from_ts = Some(ts.to_string()); }
                }
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { to_ts = Some(ts.to_string()); }
            }
            lines += 1;
        }
    }
    let sha = format!("sha256:{:x}", h.finalize());
    Ok(ComputeStats{ lines, bytes, from_ts, to_ts, sha256_file: sha })
}

pub(crate) fn chain_next(prev: Option<&str>, item: &str) -> String {
    let mut h = Sha256::new();
    if let Some(p) = prev {
        if !p.is_empty() { h.update(p.as_bytes()); h.update(&[0x0a]); }
    }
    h.update(item.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

pub(crate) fn load_events_index(log_dir: &Path) -> anyhow::Result<IndexFile> {
    let p = log_dir.join("events.index.json");
    if !p.exists() {
        return Ok(IndexFile{ schema: "events_index.v0.1".into(), segments: vec![], head: None, updated_at: None });
    }
    let txt = fs::read_to_string(&p)?;
    let mut v: IndexFile = serde_json::from_str(&txt).unwrap_or(IndexFile{ schema: "events_index.v0.1".into(), segments: vec![], head: None, updated_at: None });
    if v.schema.is_empty() { v.schema = "events_index.v0.1".into(); }
    Ok(v)
}

pub(crate) fn save_events_index(log_dir: &Path, idx: &IndexFile) -> anyhow::Result<()> {
    let p = log_dir.join("events.index.json");
    if let Some(d) = p.parent() { fs::create_dir_all(d)?; }
    fs::write(p, serde_json::to_vec_pretty(idx)?)?;
    Ok(())
}

fn run_cli_json(cwd: &Path, args: &[&str]) -> anyhow::Result<(i32, serde_json::Value)> {
    let exe = std::env::current_exe()?;
    let mut a: Vec<&str> = args.to_vec();
    if !a.iter().any(|x| *x == "--output") {
        a.push("--output"); a.push("json");
    }
    let out = Proc::new(exe).current_dir(cwd).args(&a).output()?;
    let code = out.status.code().unwrap_or(-1);
    match parse_json_resilient(&out.stdout) {
        Some(val) => Ok((code, val)),
        None => {
            let stdout_head = String::from_utf8_lossy(&out.stdout).chars().take(300).collect::<String>();
            let stderr_head = String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>();
            Err(anyhow::anyhow!(
                "stdout not JSON for {:?}; code={}. stdout_head={:?} stderr_head={:?}",
                a, code, stdout_head, stderr_head
            ))
        }
    }
}

fn parse_json_resilient(bytes: &[u8]) -> Option<serde_json::Value> {
    // Strip UTF-8 BOM if present
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) { &bytes[3..] } else { bytes };
    // Fast path
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) { return Some(v); }
    // Trim whitespace and retry
    let s = String::from_utf8_lossy(bytes);
    let trimmed = s.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) { return Some(v); }
    // Try last JSON-looking line (tolerate benign logs before JSON)
    for line in trimmed.lines().rev() {
        let l = line.trim();
        if l.starts_with('{') || l.starts_with('[') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) { return Some(v); }
        }
    }
    None
}

fn run_cli_text(cwd: &Path, args: &[&str]) -> anyhow::Result<(i32, String)> {
    let exe = std::env::current_exe()?;
    let out = Proc::new(exe).current_dir(cwd).args(args).output()?;
    let code = out.status.code().unwrap_or(-1);
    Ok((code, String::from_utf8_lossy(&out.stdout).to_string()))
}

// ===== plans verify & index helpers (PLAN-50) =====

fn verify_plans(root: &Path, base: Option<&Path>) -> anyhow::Result<PlansVerifyResult> {
    let project = base.map(|b| root.join(b)).unwrap_or_else(|| root.join("docs/project"));
    let mut items = Vec::<PlansVerifyItem>::new();
    let mut all_ok = true;

    // current
    {
        let dir = project.join("current");
        let mut i = PlansVerifyItem { name: "current".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
        if dir.exists() {
            let files: Vec<_> = std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension()==Some(OsStr::new("md")))
                .filter(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase().starts_with("plan-")).unwrap_or(false))
                .collect();
            i.count = Some(files.len() as u64);
            if files.len() != 1 {
                i.ok = false;
                i.errors.push(format!("expected exactly 1 plan, found {}", files.len()));
            }
        } else {
            i.ok = false;
            i.errors.push("folder missing".into());
        }
        all_ok &= i.ok;
        items.push(i);
    }

    // next
    {
        let dir = project.join("next");
        let mut i = PlansVerifyItem { name: "next".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
        if dir.exists() {
            let mut ids = std::collections::HashMap::<String, Vec<String>>::new();
            let mut total: u64 = 0;
            for e in std::fs::read_dir(&dir)? {
                let p = e?.path();
                if p.extension()!=Some(OsStr::new("md")) { continue; }
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if !fname.to_lowercase().starts_with("plan-") { continue; }
                total += 1;
                if fname.starts_with("plan-") {
                    i.warnings.push(format!("legacy lowercase name: {}", fname));
                }
                let (pid, _slug) = derive_id_slug(&p);
                if let Some(id) = pid {
                    ids.entry(id).or_default().push(fname);
                }
            }
            i.count = Some(total);
            for (id, files) in ids {
                if files.len() > 1 {
                    i.ok = false;
                    i.errors.push(format!("duplicate plan_id {} in next/: {}", id, files.join(", ")));
                }
            }
        }
        all_ok &= i.ok;
        items.push(i);
    }

    // done (warn-only immutability check placeholder)
    {
        let dir = project.join("done");
        let mut i = PlansVerifyItem { name: "done".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
        if dir.exists() {
            let cnt = std::fs::read_dir(&dir)?.filter_map(|e| e.ok()).filter(|e| e.path().extension()==Some(OsStr::new("md"))).count();
            i.count = Some(cnt as u64);
        }
        items.push(i);
    }

    // backlog (warn on numbered)
    {
        let dir = project.join("backlog");
        let mut i = PlansVerifyItem { name: "backlog".into(), ok: true, errors: vec![], warnings: vec![], touched: vec![], count: None };
        if dir.exists() {
            let mut total: u64 = 0;
            for e in std::fs::read_dir(&dir)? {
                let p = e?.path();
                if p.extension()!=Some(OsStr::new("md")) { continue; }
                total += 1;
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if fname.starts_with("PLAN-") {
                    i.warnings.push(format!("numbered plan in backlog: {}", fname));
                }
            }
            i.count = Some(total);
        }
        items.push(i);
    }

    // archived, paused, dumped (informational)
    for name in ["archived","paused","dumped"] {
        let dir = project.join(name);
        let cnt = if dir.exists() {
            std::fs::read_dir(&dir)?.filter_map(|e| e.ok()).filter(|e| e.path().extension()==Some(OsStr::new("md"))).count()
        } else { 0 };
        items.push(PlansVerifyItem {
            name: name.into(),
            ok: true,
            errors: vec![],
            warnings: vec![],
            touched: vec![],
            count: Some(cnt as u64),
        });
    }

    let summary = if all_ok { "plans OK".into() } else { "plans violations present".into() };
    Ok(PlansVerifyResult { ok: all_ok, summary, items })
}

fn index_build(root: &Path, base: Option<&Path>, dry: bool) -> anyhow::Result<PlansIndexResult> {
    let project = base.map(|b| root.join(b)).unwrap_or_else(|| root.join("docs/project"));
    let mut touched = Vec::<String>::new();
    let mut warnings = Vec::<String>::new();

    let folders = ["next","current","done","backlog","archived","paused","dumped"];
    for name in folders {
        let folder = project.join(name);
        let readme = folder.join("README.md");
        if !readme.exists() {
            warnings.push(format!("missing README for {}", name));
            continue;
        }
        let links = render_index_for_folder(&folder, name)?;
        let changed = update_auto_block(&readme, "plans-index", &links, dry)?;
        if changed {
            touched.push(readme.display().to_string());
        }
    }

    Ok(PlansIndexResult { ok: true, touched, warnings })
}

fn render_index_for_folder(folder: &Path, name: &str) -> anyhow::Result<String> {
    let mut entries: Vec<(String,String)> = Vec::new(); // (display, link)
    if !folder.exists() {
        return Ok("_No plan files currently listed in this folder._".into());
    }
    let mut files: Vec<_> = std::fs::read_dir(folder)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension()==Some(OsStr::new("md")))
        .collect();
    files.sort();

    match name {
        "next" => {
            for p in files {
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if !fname.to_lowercase().starts_with("plan-") { continue; }
                entries.push((fname.clone(), format!("./{}", fname)));
            }
        }
        "current" => {
            for p in files {
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if !fname.to_lowercase().starts_with("plan-") { continue; }
                entries.push((fname.clone(), format!("./{}", fname)));
            }
        }
        "done" => {
            for p in files {
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if !fname.to_lowercase().starts_with("plan-") { continue; }
                entries.push((fname.clone(), format!("./{}", fname)));
            }
        }
        "backlog" => {
            for p in files {
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if fname.starts_with("PLAN-") { continue; }
                // list plan-like docs (non-numbered)
                entries.push((fname.clone(), format!("./{}", fname)));
            }
        }
        _ => {
            for p in files {
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if !fname.to_lowercase().starts_with("plan-") { continue; }
                entries.push((fname.clone(), format!("./{}", fname)));
            }
        }
    }

    if entries.is_empty() {
        Ok("_No plan files currently listed in this folder._".into())
    } else {
        let mut out = String::new();
        for (disp, link) in entries {
            out.push_str(&format!("- [{}]({})\n", disp, link));
        }
        Ok(out.trim_end().to_string())
    }
}

fn update_auto_block(readme: &Path, id: &str, body: &str, dry: bool) -> anyhow::Result<bool> {
    let txt = std::fs::read_to_string(readme)?;
    let begin = format!("<!-- BEGIN: AUTO (id={}) -->", id);
    let end = "<!-- END: AUTO -->";
    let (start_idx, end_idx) = match (txt.find(&begin), txt.find(end)) {
        (Some(s), Some(e)) if e > s => (s, e),
        _ => return Ok(false), // no block; do not modify
    };
    // find end of begin line
    let after_begin = start_idx + begin.len();
    let before_end = end_idx;
    let mut new_txt = String::new();
    new_txt.push_str(&txt[..after_begin]);
    new_txt.push('\n');
    new_txt.push_str(body);
    new_txt.push('\n');
    new_txt.push_str(&txt[before_end..]);

    if new_txt == txt {
        return Ok(false);
    }
    if !dry {
        std::fs::write(readme, new_txt.as_bytes())?;
    }
    Ok(true)
}
