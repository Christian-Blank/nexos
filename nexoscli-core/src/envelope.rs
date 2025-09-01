use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub run_id: String,
    pub plugin: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorItem {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope<T: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub errors: Vec<ErrorItem>,
    pub meta: Meta,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(run_id: &str, plugin: &str, command: &str, data: T, duration_ms: Option<u64>) -> Self {
        Self {
            data: Some(data),
            errors: vec![],
            meta: Meta {
                run_id: run_id.to_string(),
                plugin: plugin.to_string(),
                command: command.to_string(),
                duration_ms,
            },
        }
    }
}

impl Envelope<serde_json::Value> {
    pub fn err(
        run_id: &str,
        plugin: &str,
        command: &str,
        code: &str,
        message: &str,
        suggestion: Option<String>,
        duration_ms: Option<u64>,
    ) -> Self {
        Self {
            data: None,
            errors: vec![ErrorItem {
                code: code.to_string(),
                message: message.to_string(),
                suggestion,
            }],
            meta: Meta {
                run_id: run_id.to_string(),
                plugin: plugin.to_string(),
                command: command.to_string(),
                duration_ms,
            },
        }
    }
}
