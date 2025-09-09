use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct NormalizeArgs {
    #[arg(long)]
    pub overwrite: bool,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long = "task-id")]
    pub task_id: Option<String>,
}

pub fn subcommand() -> Command {
    NormalizeArgs::augment_args(
        Command::new("normalize")
            .about("Normalize Cline tasks → AIM streams (.nexoscli/agents/streams/Cline/<task>/messages.ndjson)"),
    )
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = NormalizeArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let dot = std::env::current_dir()?.join(".nexoscli");
    let since_ts = super::parse_since(args.since.as_ref());
    let sum = crate::normalize::normalize_since(&dot, since_ts, args.overwrite, args.task_id.as_deref())?;
    Ok(ctx.ok("cline", "normalize", &serde_json::to_value(sum)?))
}
