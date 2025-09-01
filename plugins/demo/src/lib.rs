use clap::{Arg, ArgMatches, Command};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};

use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct HelloArgs {
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct HelloResult {
    message: String,
}

pub struct DemoPlugin;
impl DemoPlugin {
    pub fn new() -> Self { Self }
}

impl Plugin for DemoPlugin {
    fn name(&self) -> &'static str { "demo" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Demo plugin" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("demo")
              .about("Demo plugin")
              .subcommand(
                 Command::new("hello")
                   .about("Say hello from demo plugin")
                   .arg(Arg::new("name").long("name").required(false).value_name("TEXT"))
              )
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("hello", m)) => {
                let name = m.get_one::<String>("name").cloned().unwrap_or_else(|| "world".to_string());
                let data = HelloResult { message: format!("hello, {}", name) };
                let exit = ctx.ok("demo", "hello", &serde_json::json!({ "message": data.message }));
                Ok(exit)
            }
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "demo.hello".to_string(),
            about: "Say hello".to_string(),
            args_schema: schema_for!(HelloArgs),
            result_schema: schema_for!(HelloResult),
        }]
    }
}
