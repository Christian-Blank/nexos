use clap::{Arg, ArgAction, ArgMatches, Command};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::{span, Level};

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DevInitArgs {
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    yes: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DevInitResult {
    planned_actions: Vec<String>,
    applied: bool,
}

pub struct DevPlugin;
impl DevPlugin { pub fn new() -> Self { Self } }

impl Plugin for DevPlugin {
    fn name(&self) -> &'static str { "dev" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Developer ergonomics (init, generators)" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("dev").about("Developer utilities")
              .subcommand(
                Command::new("init")
                  .about("Initialize .nexoscli/ (Plan/Apply, idempotent)")
                  .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue).help("Show plan only"))
                  .arg(Arg::new("yes").long("yes").action(ArgAction::SetTrue).help("Apply without prompt"))
                  .arg(Arg::new("with-config").long("with-config").action(ArgAction::SetTrue).help("Create .nexoscli/config.toml with defaults"))
              )
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("init", m)) => {
                let dry_run = m.get_flag("dry-run");
                let yes = m.get_flag("yes");
                let with_config = m.get_flag("with-config");
                let run = span!(Level::INFO, "dev.init", dry_run, yes, with_config);
                let _g = run.enter();

                let root = std::env::current_dir()?;
                let dot = root.join(".nexoscli");
                let log_dir = dot.join("log");
                let manifest = dot.join("manifest.toml");
                let gitignore = root.join(".gitignore");

                // Plan
                let mut actions: Vec<String> = vec![];
                if !dot.exists() { actions.push(format!("create_dir {}", dot.display())); }
                if !log_dir.exists() { actions.push(format!("create_dir {}", log_dir.display())); }
                if !manifest.exists() { actions.push(format!("write_file {}", manifest.display())); }
                if needs_gitignore_line(&gitignore, ".nexoscli/log/")? {
                    actions.push(format!("append_gitignore {}", gitignore.display()));
                }
                if with_config {
                    let cfg = dot.join("config.toml");
                    if !cfg.exists() {
                        actions.push(format!("write_file {}", cfg.display()));
                    }
                }

                if dry_run && !yes {
                    let exit = ctx.ok("dev", "init.plan", &serde_json::json!({ "actions": actions }));
                    return Ok(exit);
                }

                // Apply
                if !dot.exists() { fs::create_dir_all(&dot)?; }
                if !log_dir.exists() { fs::create_dir_all(&log_dir)?; }
                if !manifest.exists() {
                    let repo = root.file_name().and_then(|s| s.to_str()).unwrap_or("repo");
                    let ts = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
                    let content = format!(
                        "[project]\nname = \"{}\"\nversion = \"0.1.0\"\ncreated = \"{}\"\n",
                        repo, ts
                    );
                    fs::write(&manifest, content)?;
                }
                if needs_gitignore_line(&gitignore, ".nexoscli/log/")? {
                    append_gitignore_line(&gitignore, ".nexoscli/log/")?;
                }
                if with_config {
                    let cfg = dot.join("config.toml");
                    if !cfg.exists() {
                        let content = r#"
[output]
default = "json"

[telemetry]
enabled = false
endpoint = ""
"#;
                        fs::write(&cfg, content.trim_start())?;
                    }
                }

                let exit = ctx.ok("dev", "init.apply", &serde_json::json!({ "actions": actions }));
                Ok(exit)
            }
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec{
            name: "dev.init".to_string(),
            about: "Initialize .nexoscli/ (Plan/Apply, idempotent)".to_string(),
            args_schema: schema_for!(DevInitArgs),
            result_schema: schema_for!(DevInitResult),
        }]
    }
}

fn needs_gitignore_line(path: &PathBuf, line: &str) -> std::io::Result<bool> {
    if !path.exists() { return Ok(true); }
    let content = std::fs::read_to_string(path)?;
    Ok(!content.lines().any(|l| l.trim() == line.trim()))
}

fn append_gitignore_line(path: &PathBuf, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::fs::OpenOptions;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    if needs_gitignore_line(path, line)? {
        if f.metadata()?.len() > 0 { write!(f, "\n")?; }
        write!(f, "{}\n", line)?;
    }
    Ok(())
}
