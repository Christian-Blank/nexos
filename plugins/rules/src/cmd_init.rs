use clap::{ArgMatches, Args, Command, FromArgMatches};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct RulesInitArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RulesInitResult {
    pub written: String,
}

pub fn subcommand() -> Command {
    RulesInitArgs::augment_args(Command::new("init").about("Create minimal rules pack"))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args =
        RulesInitArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let root = std::env::current_dir()?;
    super::init_rules(&root, args.force)?;
    let written = root.join("rules").to_string_lossy().to_string();
    let exit = ctx.ok(
        "rules",
        "init",
        &RulesInitResult {
            written,
        },
    );
    Ok(exit)
}
