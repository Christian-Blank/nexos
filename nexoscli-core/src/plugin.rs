use clap::{ArgMatches, Command};
use schemars::schema::RootSchema;
use serde::Serialize;

use crate::Result;
use crate::providers::Providers;
use crate::event::EventWriter;
use crate::envelope::Envelope;

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

    /// Render & event-write a success envelope in one go.
    pub fn ok<T: serde::Serialize>(&self, plugin: &str, command: &str, data: T) -> i32 {
        // write canonical event first
        let _ = self.events.write(plugin, command, &data, 0);

        // render a stable envelope
        let env = Envelope::ok(&self.providers.run_id, plugin, command, data);
        match self.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
            OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&env).unwrap()),
            OutputFormat::Text => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
        }
        0
    }

    /// Render & event-write a failure envelope—use for recoverable/expected errors.
    pub fn err(
        &self,
        plugin: &str,
        command: &str,
        code: &str,
        message: &str,
        suggestion: Option<&str>,
        exit_code: i32,
    ) -> i32 {
        let env = Envelope::err(
            &self.providers.run_id,
            plugin,
            command,
            code,
            message,
            suggestion.map(|s| s.to_string()),
        );
        let _ = self.events.write(
            plugin,
            command,
            &serde_json::json!({"error": { "code": code, "message": message }}),
            exit_code,
        );

        match self.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
            OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&env).unwrap()),
            OutputFormat::Text => println!("{}", serde_json::to_string_pretty(&env).unwrap()),
        }
        exit_code
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
