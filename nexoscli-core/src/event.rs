use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

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
    pub content_hash: String,
}

#[derive(Clone)]
pub struct EventWriter {
    providers: Arc<Providers>,
}

impl EventWriter {
    pub fn new(providers: Arc<Providers>) -> Self { Self { providers } }

    /// Write a run event with optional error_code and duration.
    pub fn write(
        &self,
        plugin: &str,
        command: &str,
        content: &serde_json::Value,
        exit_code: i32,
        error_code: Option<&str>,
        dur_ms: Option<u64>,
    ) -> Result<()> {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(content).unwrap_or_default();
        let mut h = Sha256::new();
        h.update(&bytes);
        let content_hash = format!("sha256:{:x}", h.finalize());

        let line = serde_json::json!({
            "ts": Clock::now_rfc3339(),
            "run_id": self.providers.run_id,
            "plugin": plugin,
            "command": command,
            "exit_code": exit_code,
            "error_code": error_code,
            "dur_ms": dur_ms,
            "content_hash": content_hash,
            "data": content,
        });
        self.append_line(&line)
    }

    fn append_line(&self, line: &serde_json::Value) -> Result<()> {
        let line_str = serde_json::to_string(line)?;
        Io::append_line(&self.providers.event_log_path, &line_str)?;
        
        // Mirror as a span event (handy during dev)
        info!(target: "nexos.event", 
              plugin = line["plugin"].as_str().unwrap_or(""), 
              command = line["command"].as_str().unwrap_or(""), 
              exit_code = line["exit_code"].as_i64().unwrap_or(0),
              content_hash = line["content_hash"].as_str().unwrap_or(""),
              "event_written");
        Ok(())
    }
}
