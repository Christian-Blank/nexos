use clap::{Arg, ArgAction, ArgMatches, Command};
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExampleArgs {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub loud: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExampleResult {
    pub message: String,
}

pub fn subcommand() -> Command {
    Command::new("example")
        .about("Example subcommand (blueprint)")
        .arg(Arg::new("name").long("name").value_name("TEXT"))
        .arg(Arg::new("loud").long("loud").action(ArgAction::SetTrue))
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let name = m.get_one::<String>("name").cloned().unwrap_or_else(|| "world".to_string());
    let loud = m.get_flag("loud");
    let msg = if loud { format!("HELLO, {}!", name.to_uppercase()) } else { format!("hello, {}", name) };
    let exit = ctx.ok("{{plugin_name}}","example",&serde_json::json!({ "message": msg }));
    Ok(exit)
}
