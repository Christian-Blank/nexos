use std::sync::Arc;
use clap::{ArgMatches, Command};

use crate::{Result};
use crate::providers::Providers;
use crate::event::EventWriter;

#[derive(Clone, Copy, Debug)]
pub enum OutputFormat { Text, Json, Yaml }

pub struct CommandContext {
    pub providers: Arc<Providers>,
    pub events: EventWriter,
    pub verbosity: u8,
    pub output: OutputFormat,
}

impl CommandContext {
    pub fn new(providers: Arc<Providers>, verbosity: u8, output: OutputFormat) -> Self {
        let events = EventWriter::new(providers.clone());
        Self { providers, events, verbosity, output }
    }

    pub fn render<T: serde::Serialize>(&self, data: &T) {
        match self.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(data).unwrap()),
            OutputFormat::Yaml => println!("{}", serde_yaml::to_string(data).unwrap()),
            OutputFormat::Text => {
                // default fallback for text: JSON dump for now
                println!("{}", serde_json::to_string_pretty(data).unwrap());
            }
        }
    }
}

pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn describe(&self) -> &'static str;

    fn augment_commands(&self, app: Command) -> Command;

    /// sub_matches are the matches for this plugin's root subcommand
    fn execute(&self, sub_matches: &ArgMatches, ctx: &CommandContext) -> Result<i32>;
}
