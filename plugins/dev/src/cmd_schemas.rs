use clap::{ArgMatches, Command};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

/// Stub module for `dev schemas` (pilot footprint)
pub fn subcommand() -> Command {
    Command::new("schemas").about("Export contract JSON Schemas (stub)")
}

pub fn run(_m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    Ok(ctx.ok("dev","stub",&serde_json::json!({ "stub": true })))
}
