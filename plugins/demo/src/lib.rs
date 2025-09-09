use clap::{ArgMatches, Command};
use schemars::schema_for;
use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

mod cmd_hello;

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
              .subcommand(cmd_hello::subcommand())
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("hello", m)) => cmd_hello::run(m, ctx),
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "demo.hello".to_string(),
            about: "Say hello".to_string(),
            args_schema: schema_for!(cmd_hello::HelloArgs),
            result_schema: schema_for!(cmd_hello::HelloResult),
        }]
    }
}
