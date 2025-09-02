use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::providers::{Clock, Io, Providers};
use crate::{Result, ledger::Ledger};

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
    ledger: Option<Arc<Mutex<Ledger>>>,
    ledger_enabled: bool,
}

impl EventWriter {
    pub fn new(providers: Arc<Providers>) -> Self {
        let ledger_enabled = std::env::var("NEXOS_LEDGER").ok().as_deref() != Some("0");
        let ledger = if ledger_enabled {
            Ledger::open(".nexoscli", &providers.run_id).ok().map(|l| Arc::new(Mutex::new(l)))
        } else {
            None
        };
        Self { providers, ledger, ledger_enabled }
    }

    /// Write a run event with optional error_code, duration, and context.
    pub fn write(
        &self,
        plugin: &str,
        command: &str,
        content: &serde_json::Value,
        exit_code: i32,
        error_code: Option<&str>,
        dur_ms: Option<u64>,
        context: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Result<()> {
        let ts = Clock::now_rfc3339();

        let mut line = serde_json::json!({
            "ts": ts,
            "run_id": self.providers.run_id,
            "plugin": plugin,
            "command": command,
            "exit_code": exit_code,
            "error_code": error_code,
            "dur_ms": dur_ms,
            "data": content,
        });

        // Add context if present
        if let Some(ctx) = context {
            line["context"] = serde_json::Value::Object(ctx.clone());
        }

        if self.ledger_enabled {
            if let Some(ref led) = self.ledger {
                // canonical subset for stable hashing
                let canonical = serde_json::json!({
                    "plugin": plugin,
                    "command": command,
                    "exit_code": exit_code,
                    "dur_ms": dur_ms,
                    "data": content
                });
                let mut guard = led.lock().unwrap();
                // best-effort; if ledger append fails, still write the base line
                if let Err(e) = guard.append_event(&ts, &canonical, &mut line) {
                    tracing::warn!("ledger.append_event failed: {}", e);
                }
            }
            // When ledger is enabled, it already wrote the line to the global log.
            // Emit span for observability but skip duplicate file append.
            info!(target: "nexos.event", 
                  plugin = line["plugin"].as_str().unwrap_or(""), 
                  command = line["command"].as_str().unwrap_or(""), 
                  exit_code = line["exit_code"].as_i64().unwrap_or(0),
                  content_hash = line["content_hash"].as_str().unwrap_or(""),
                  "event_written");
            Ok(())
        } else {
            // Legacy v0.2 behavior: compute content_hash over content only
            use sha2::{Digest, Sha256};
            let bytes = serde_json::to_vec(content).unwrap_or_default();
            let mut h = Sha256::new();
            h.update(&bytes);
            let content_hash = format!("sha256:{:x}", h.finalize());
            line["content_hash"] = serde_json::Value::String(content_hash);
            self.append_line(&line)
        }
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
