use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Args)]
pub struct IssueListArgs {
    #[arg(long, value_parser = ["open", "closed", "all"], default_value = "all")]
    pub state: String, // "open" | "closed" | "all"
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueListResult {
    pub items: Vec<super::Issue>,
}

pub fn subcommand() -> Command {
    IssueListArgs::augment_args(Command::new("list").about("List issues"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = IssueListArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let root = std::env::current_dir()?;
    let dir = root.join(".nexoscli").join("issues");
    let mut items: Vec<super::Issue> = Vec::new();

    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
                let bytes = fs::read(entry.path())?;
                if let Ok(iss) = serde_json::from_slice::<super::Issue>(&bytes) {
                    items.push(iss);
                }
            }
        }
    }

    let filtered: Vec<super::Issue> = match args.state.as_str() {
        "open" => items
            .into_iter()
            .filter(|i| matches!(i.state, super::IssueState::Open))
            .collect(),
        "closed" => items
            .into_iter()
            .filter(|i| matches!(i.state, super::IssueState::Closed))
            .collect(),
        _ => items,
    };

    let result = IssueListResult { items: filtered };
    Ok(ctx.ok("issue", "list", &result))
}
