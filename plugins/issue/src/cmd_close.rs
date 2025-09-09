use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Args)]
pub struct CloseArgs {
    #[arg(value_name = "ID")]
    pub id: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CloseResult {
    pub id: String,
    pub state: String,
}

pub fn subcommand() -> Command {
    CloseArgs::augment_args(Command::new("close").about("Close an issue"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = CloseArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let root = std::env::current_dir()?;
    let path = root
        .join(".nexoscli")
        .join("issues")
        .join(format!("{}.json", args.id));
    let mut issue: super::Issue = serde_json::from_slice(&fs::read(&path)?)?;
    issue.state = super::IssueState::Closed;
    issue.updated_at = super::now();
    fs::write(&path, serde_json::to_vec_pretty(&issue)?)?;
    let res = CloseResult {
        id: issue.id,
        state: "closed".to_string(),
    };
    Ok(ctx.ok("issue", "close", &res))
}
