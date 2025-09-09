use clap::{ArgMatches, Command, FromArgMatches, Args};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Serialize, Deserialize, JsonSchema, clap::Args, Debug, Clone)]
pub struct HelloArgs {
    /// Optional recipient name
    #[arg(long = "name")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct HelloResult {
    pub message: String,
}

pub fn subcommand() -> Command {
    // Build flags from derives so FromArgMatches works reliably.
    HelloArgs::augment_args(
        Command::new("hello")
            .about("Say hello from demo plugin"),
    )
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = HelloArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let name = args.name.unwrap_or_else(|| "world".to_string());
    let data = HelloResult { message: format!("hello, {}", name) };
    let exit = ctx.ok("demo", "hello", &serde_json::json!({ "message": data.message }));
    Ok(exit)
}
