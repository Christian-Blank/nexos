use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use hex::encode as hex_encode;
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

    pub fn write<T: serde::Serialize>(&self, plugin: &str, command: &str, data: &T, exit_code: i32) -> Result<()> {
        let payload = serde_json::to_vec(data)?;
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        let content_hash = format!("sha256:{}", hex_encode(hasher.finalize()));

        let ev = Event {
            ts: Clock::now_rfc3339(),
            run_id: &self.providers.run_id,
            plugin,
            command,
            data,
            exit_code,
            content_hash: content_hash.clone(),
        };
        let line = serde_json::to_string(&ev)?;
        Io::append_line(&self.providers.event_log_path, &line)?;

        // Mirror as a span event (handy during dev)
        info!(target: "nexos.event", plugin, command, exit_code, %content_hash, "event_written");
        Ok(())
    }
}
