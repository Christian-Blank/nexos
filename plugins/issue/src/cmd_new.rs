use clap::{ArgMatches, Args, Command, FromArgMatches};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Args)]
pub struct IssueNewArgs {
    #[arg(long, short = 't')]
    pub title: String,
    #[arg(long)]
    pub body: Option<String>,
    // Accept CSV of labels: --labels bug,help wanted
    #[arg(long, value_delimiter = ',')]
    pub labels: Option<Vec<String>>,
}

pub fn subcommand() -> Command {
    IssueNewArgs::augment_args(Command::new("new").about("Create a new issue"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = IssueNewArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let root = std::env::current_dir()?;
    let dir = root.join(".nexoscli").join("issues");
    std::fs::create_dir_all(&dir)?;

    let id = super::next_id(&dir)?;
    let now_s = super::now();
    let issue = super::Issue {
        id: id.clone(),
        title: args.title,
        body: args.body,
        labels: args.labels.unwrap_or_default(),
        state: super::IssueState::Open,
        created_at: now_s.clone(),
        updated_at: now_s,
    };

    let path = dir.join(format!("{id}.json"));
    fs::write(&path, serde_json::to_vec_pretty(&issue)?)?;

    Ok(ctx.ok("issue", "new", &issue))
}
