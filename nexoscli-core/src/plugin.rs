use clap::{ArgMatches, Command};
use schemars::{schema::RootSchema, JsonSchema};
use serde::{Serialize, Deserialize};
use std::time::Instant;

use crate::Result;
use crate::providers::Providers;
use crate::event::EventWriter;
use crate::envelope::Envelope;
use crate::config::Config;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat { Text, Json, Yaml }

pub struct CommandContext {
    pub config: std::sync::Arc<Config>,
    pub providers: std::sync::Arc<Providers>,
    pub events: EventWriter,
    pub verbosity: u8,
    pub output: OutputFormat,
    pub started_at: Instant,
}

impl CommandContext {
    pub fn new(
        providers: std::sync::Arc<Providers>,
        config: std::sync::Arc<Config>,
        verbosity: u8,
        output: OutputFormat,
    ) -> Self {
        let events = EventWriter::new(providers.clone());
        Self { 
            config, 
            providers, 
            events, 
            verbosity, 
            output,
            started_at: Instant::now(),
        }
    }

    fn dur_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    fn active_task_map(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        let current_file = std::path::PathBuf::from(".nexoscli").join("task").join("current");
        let id = std::fs::read_to_string(&current_file).ok()?.trim().to_string();
        if id.is_empty() { return None; }
        
        let ledger = std::path::PathBuf::from(".nexoscli").join("tasks").join(&id).join("task.json");
        let stage = std::fs::read_to_string(&ledger).ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("stage").and_then(|x| x.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| "implementation".to_string());
        
        let mut m = serde_json::Map::new();
        m.insert("task_id".into(), serde_json::Value::String(id));
        m.insert("stage".into(), serde_json::Value::String(stage));
        Some(m)
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
        let payload = serde_json::to_value(&data).unwrap_or(serde_json::json!({}));
        let dur = self.dur_ms();
        let context_obj = self.active_task_map();
        let _ = self.events.write(plugin, command, &payload, 0, None, Some(dur), context_obj.as_ref());

        // render a stable envelope
        let env = Envelope::ok(&self.providers.run_id, plugin, command, data, Some(dur), context_obj);
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
        let dur = self.dur_ms();
        let context_obj = self.active_task_map();
        let payload = serde_json::json!({"error": { "code": code, "message": message, "hint": suggestion }});
        let _ = self.events.write(plugin, command, &payload, exit_code, Some(code), Some(dur), context_obj.as_ref());

        let env = Envelope::err(
            &self.providers.run_id,
            plugin,
            command,
            code,
            message,
            suggestion.map(|s| s.to_string()),
            Some(dur),
            context_obj,
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
