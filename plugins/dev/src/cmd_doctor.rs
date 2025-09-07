use clap::{ArgMatches, Command};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

/// Stub module for `dev doctor` (pilot footprint)
pub fn subcommand() -> Command {
    Command::new("doctor").about("Environment & hygiene checks (stub)")
}

pub fn run(_m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    Ok(ctx.ok("dev","stub",&serde_json::json!({ "stub": true })))
}
