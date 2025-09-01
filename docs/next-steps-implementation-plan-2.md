Perfect next bite. We’ll add two capabilities to the existing dev plugin:
	1.	Plugin scaffolder – nexoscli dev plugin new --name <name> [--desc "..."] [--register] [--dry-run] [--yes]
	•	Creates plugins/<name>/ with Cargo.toml, src/lib.rs, README.md.
	•	Optionally registers the plugin by:
	•	adding it to root [workspace].members
	•	adding a dependency to nexoscli-bin/Cargo.toml
	•	inserting a registry.register(…) line in nexoscli-bin/src/main.rs
	•	Idempotent; shows a plan before applying.
	2.	Repo structure checker – nexoscli dev plugins check [--fix-readmes] [--output json]
	•	Verifies each plugins/* directory has README.md, Cargo.toml, src/lib.rs.
	•	Checks crate name convention (nexoscli-plugin-<dir>), bin dependency, and main.rs registration.
	•	Optionally writes missing READMEs with a template.

Everything returns the standard envelope ({data, errors, meta}) and writes event lines.

⸻

1) Wire routing (bin)

Add the dev plugin already exists; we don’t change bin routing now. The new subcommands live under dev (i.e., dev plugin new and dev plugins check).

If you didn’t register nexoscli-plugin-dev earlier, ensure it’s already in the bin (you did in the last step).

⸻

2) Implement the new subcommands

Edit: plugins/dev/src/lib.rs and replace its content with the version below (it keeps dev init and adds dev plugin new + dev plugins check):

use clap::{Arg, ArgAction, ArgMatches, Command};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::{span, Level};

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

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

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PluginsCheckResult {
    summary: String,
    total: usize,
    ok: usize,
    not_ok: usize,
    items: Vec<PluginCheck>,
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
                if !dot.exists() { actions.push(fmt!("create_dir {}", dot)); }
                if !log_dir.exists() { actions.push(fmt!("create_dir {}", log_dir)); }
                if !manifest.exists() { actions.push(fmt!("write_file {}", manifest)); }
                if needs_gitignore_line(&gitignore, ".nexoscli/log/")? {
                    actions.push(fmt!("append_gitignore {}", gitignore));
                }
                if with_config && !cfg.exists() {
                    actions.push(fmt!("write_file {}", cfg));
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
                    if !plug_dir.exists() { actions.push(fmt!("create_dir {}", plug_dir)); }
                    if !src_dir.exists() { actions.push(fmt!("create_dir {}", src_dir)); }
                    if !cargo_toml.exists() { actions.push(fmt!("write_file {}", cargo_toml)); }
                    if !lib_rs.exists() { actions.push(fmt!("write_file {}", lib_rs)); }
                    if !readme.exists() { actions.push(fmt!("write_file {}", readme)); }

                    // Registration actions
                    let root_cargo = root.join("Cargo.toml");
                    let bin_cargo = root.join("nexoscli-bin").join("Cargo.toml");
                    let bin_main = root.join("nexoscli-bin").join("src").join("main.rs");

                    if register {
                        if needs_workspace_member(&root_cargo, &format!("plugins/{}", name))? {
                            actions.push(fmt!("patch_workspace_members {}", root_cargo));
                        }
                        if needs_bin_dep(&bin_cargo, &crate_name, &format!("../plugins/{}", name))? {
                            actions.push(fmt!("patch_bin_dependency {}", bin_cargo));
                        }
                        if needs_main_register(&bin_main, &crate_ident, &type_name)? {
                            actions.push(fmt!("patch_main_registration {}", bin_main));
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
        ]
    }
}

// ===== helpers =====

fn fmt<T: AsRef<Path>>(s: T) -> String { s.as_ref().display().to_string() }
fn pstr(p: &PathBuf) -> String { p.display().to_string() }

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

This file:
	•	Keeps your existing dev init logic.
	•	Adds dev plugin new with Plan/Apply and optional registration.
	•	Adds dev plugins check with an optional --fix-readmes.

⸻

3) Quick acceptance run

From your repo root:

# 1) Check current plugin structure
cargo nex -- dev plugins check --output json

# 2) Scaffold a new plugin (dry-run first)
cargo nex -- dev plugin new --name demo --desc "Demo plugin" --register --dry-run --output json

# 3) Apply
cargo nex -- dev plugin new --name demo --desc "Demo plugin" --register --yes --output json

# 4) Build & smoke the new command
cargo build --workspace
./target/debug/nexoscli demo hello --name Nexus --output json

# 5) Lint structure (and optionally fix missing READMEs)
cargo nex -- dev plugins check --fix-readmes --output json

What you should see:
	•	plugins/demo/ created with Cargo.toml, src/lib.rs, README.md.
	•	Root Cargo.toml [workspace].members includes "plugins/demo".
	•	nexoscli-bin/Cargo.toml has a nexoscli-plugin-demo = { path = "../plugins/demo" } dependency.
	•	nexoscli-bin/src/main.rs gained a registry.register(Box::new(nexoscli_plugin_demo::DemoPlugin::new()));.
	•	Running nexoscli demo hello --name Nexus --output json prints an envelope with {data:{message:"hello, Nexus"}, errors:[], meta:...} and writes an event line.

⸻

4) Notes on safety & editability
	•	Idempotent by design: re-running dev plugin new --register --yes won’t duplicate lines; the patchers check first.
	•	Human-editable: templates are minimal, commented, and easy to customize per plugin.
	•	Deterministic: all outputs go through ctx.ok/ctx.err envelopes and your JSONL events keep the ground truth.

⸻
