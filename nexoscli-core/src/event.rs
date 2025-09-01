use serde::{Serialize, Deserialize};
use std::sync::Arc;

use crate::providers::{Clock, Io, Providers};
use crate::Result;

#[derive(Debug, Serialize, Deserialize)]
pub struct Event<'a, T: Serialize> {
    pub ts: String,
    pub run_id: &'a str,
    pub plugin: &'a str,
    pub command: &'a str,
    pub data: T,
    pub exit_code: i32,
}

#[derive(Clone)]
pub struct EventWriter {
    providers: Arc<Providers>,
}

impl EventWriter {
    pub fn new(providers: Arc<Providers>) -> Self { Self { providers } }

    pub fn write<T: serde::Serialize>(&self, plugin: &str, command: &str, data: &T, exit_code: i32) -> Result<()> {
        let ev = Event {
            ts: Clock::now_rfc3339(),
            run_id: &self.providers.run_id,
            plugin,
            command,
            data,
            exit_code,
        };
        let line = serde_json::to_string(&ev)?;
        Io::append_line(&self.providers.event_log_path, &line)?;
        Ok(())
    }
}
