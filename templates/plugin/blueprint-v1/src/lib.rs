use clap::{Command, ArgMatches};
use schemars::{JsonSchema, schema_for};
use serde::{Serialize, Deserialize};
use nexoscli_core::plugin::{CommandContext, Plugin, ToolSpec};
use nexoscli_core::Result;

mod cmd_example;

pub struct {{type_name}};
impl {{type_name}} { pub fn new() -> Self { Self } }

impl Plugin for {{type_name}} {
    fn name(&self) -> &'static str { "{{plugin_name}}" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "{{desc}}" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("{{plugin_name}}")
              .about("{{desc}}")
              .subcommand(cmd_example::subcommand())
        )
    }

    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match sub_matches.subcommand() {
            Some(("example", m)) => cmd_example::run(m, ctx),
            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: "{{plugin_name}}.example".to_string(),
            about: "Example command (blueprint)".to_string(),
            args_schema: schema_for!(cmd_example::ExampleArgs),
            result_schema: schema_for!(cmd_example::ExampleResult),
        }]
    }
}
