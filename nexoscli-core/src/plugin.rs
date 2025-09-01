use clap::{ArgMatches, Command};
use schemars::{schema::RootSchema, JsonSchema};
use serde::Serialize;

use crate::Result;
use crate::providers::Providers;
use crate::event::EventWriter;

#[derive(Clone, Copy, Debug)]
pub enum OutputFormat { Text, Json, Yaml }

pub struct CommandContext {
    pub providers: std::sync::Arc<Providers>,
    pub events: EventWriter,
    pub verbosity: u8,
    pub output: OutputFormat,
}

impl CommandContext {
    pub fn new(providers: std::sync::Arc<Providers>, verbosity: u8, output: OutputFormat) -> Self {
        let events = EventWriter::new(providers.clone());
        Self { providers, events, verbosity, output }
    }

    pub fn render<T: serde::Serialize>(&self, data: &T) {
        match self.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(data).unwrap()),
            OutputFormat::Yaml => println!("{}", serde_yaml::to_string(data).unwrap()),
            OutputFormat::Text => println!("{}", serde_json::to_string_pretty(data).unwrap()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ToolSpec {
    pub name: String,            // e.g., "issue.new"
    pub about: String,           // human hint
    pub args_schema: RootSchema,
    pub result_schema: RootSchema,
}

pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn describe(&self) -> &'static str;

    fn augment_commands(&self, app: Command) -> Command;
    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32>;

    fn tool_specs(&self) -> Vec<ToolSpec> { vec![] }
}
