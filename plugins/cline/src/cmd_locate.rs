use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

 // Re-use LocateResult from the parent module so ToolSpec schemas stay consistent.
use super::LocateResult;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct LocateArgs {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub limit: Option<u64>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub storage: Option<String>,
}

pub fn subcommand() -> Command {
    LocateArgs::augment_args(Command::new("locate").about("Find Cline storage (globalStorage/workspace)"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let _args = LocateArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let (cands, selected, hints) = super::locate_all();
    let result = LocateResult {
        selected: selected.clone(),
        candidates: cands,
        hints,
    };
    Ok(ctx.ok("cline", "locate", &result))
}
