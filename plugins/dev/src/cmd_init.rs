use clap::{Arg, ArgAction, ArgMatches, Command, Args as ClapArgs, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct DevInitArgs {
    #[arg(long = "dry-run")]
    pub dry_run: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long = "with-config")]
    pub with_config: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DevInitResult {
    pub planned_actions: Vec<String>,
    pub applied: bool,
}

/// dev init subcommand (module form)
pub fn subcommand() -> Command {
    // Use the ClapArgs-derived schema to define flags so FromArgMatches works reliably.
    DevInitArgs::augment_args(
        Command::new("init")
            .about("Initialize .nexoscli/ (Plan/Apply, idempotent)")
    )
}

/// Handler for dev init (moved from lib.rs; behavior unchanged)
pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = DevInitArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let root = std::env::current_dir()?;
    let dot = root.join(".nexoscli");
    let log_dir = dot.join("log");
    let manifest = dot.join("manifest.toml");
    let gitignore: PathBuf = root.join(".gitignore");
    let cfg = dot.join("config.toml");

    let mut actions: Vec<String> = vec![];
    if !dot.exists() {
        actions.push(dot.display().to_string());
    }
    if !log_dir.exists() {
        actions.push(log_dir.display().to_string());
    }
    if !manifest.exists() {
        actions.push(manifest.display().to_string());
    }
    if super::needs_gitignore_line(&gitignore, ".nexoscli/log/")? {
        actions.push(gitignore.display().to_string());
    }
    if args.with_config && !cfg.exists() {
        actions.push(cfg.display().to_string());
    }

    if args.dry_run && !args.yes {
        let exit = ctx.ok("dev", "init.plan", &serde_json::json!({ "actions": actions }));
        return Ok(exit);
    }

    if !dot.exists() {
        fs::create_dir_all(&dot)?;
    }
    if !log_dir.exists() {
        fs::create_dir_all(&log_dir)?;
    }
    if !manifest.exists() {
        let repo = root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("repo");
        let ts = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let content = format!(
            "[project]\nname = \"{}\"\nversion = \"0.1.0\"\ncreated = \"{}\"\n",
            repo, ts
        );
        fs::write(&manifest, content)?;
    }
    if super::needs_gitignore_line(&gitignore, ".nexoscli/log/")? {
        super::append_gitignore_line(&gitignore, ".nexoscli/log/")?;
    }
    if args.with_config && !cfg.exists() {
        let content = r#"
[output]
default = "json"

[telemetry]
enabled = false
endpoint = ""
"#
        .trim_start()
        .to_string();
        fs::write(&cfg, content)?;
    }

    let exit = ctx.ok(
        "dev",
        "init.apply",
        &serde_json::json!({ "actions": actions }),
    );
    Ok(exit)
}
