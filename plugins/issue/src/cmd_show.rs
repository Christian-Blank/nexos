use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Args)]
pub struct ShowArgs {
    #[arg(value_name = "ID")]
    pub id: String,
}

pub fn subcommand() -> Command {
    ShowArgs::augment_args(Command::new("show").about("Show an issue"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = ShowArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let root = std::env::current_dir()?;
    let path = root
        .join(".nexoscli")
        .join("issues")
        .join(format!("{}.json", args.id));
    let bytes = fs::read(&path)?;
    let issue: super::Issue = serde_json::from_slice(&bytes)?;
    Ok(ctx.ok("issue", "show", &issue))
}
