use clap::{ArgMatches, Args, Command, FromArgMatches};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct RulesExportArgs {
    #[arg(long, alias = "for", value_parser = ["human","agent","cline"])]
    pub audience: String,
    #[arg(long)]
    pub out: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RulesExportResult {
    pub audience: String,
    pub out: String,
}

pub fn subcommand() -> Command {
    RulesExportArgs::augment_args(
        Command::new("export").about("Export audience-specific bundle"),
    )
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args =
        RulesExportArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let root = std::env::current_dir()?;
    let md = super::export_rules(&root, &args.audience)?;

    let out_path = args.out.unwrap_or_else(|| match args.audience.as_str() {
        "cline" => ".clinerules/compiled.md".into(),
        "human" => "docs/RULES.md".into(),
        "agent" => ".nexoscli/rules/for-agents.md".into(),
        _ => "rules_export.md".into(),
    });

    if let Some(dir) = Path::new(&out_path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&out_path, md.as_bytes())?;

    let exit = ctx.ok(
        "rules",
        "export",
        &RulesExportResult {
            audience: args.audience,
            out: out_path,
        },
    );
    Ok(exit)
}
