use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------- Schema tag constants ----------------

pub const EVENT_SCHEMA_V03: &str = "event.v0.3";
pub const TERM_RECORD_SCHEMA_V01: &str = "term_record.v0.1";
pub const FILE_WATCH_EVENT_SCHEMA_V01: &str = "file_watch_event.v0.1";
pub const BAG_MANIFEST_SCHEMA_V02: &str = "bag_manifest.v0.2";

// ---------------- Event line (v0.3) ----------------
// Represents an enriched event line as written to events.jsonl (with hash chain fields).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventLineV03 {
    #[serde(default = "schema_event_v03")]
    pub schema: &'static str,

    pub ts: String,
    pub run_id: String,

    // hash chain fields
    pub seq: u64,
    pub prev: Option<String>,
    pub content_hash: String,
    pub chain_hash: String,

    // command metadata
    pub plugin: String,
    pub command: String,
    pub args_hash: Option<String>,
    pub exit_code: i32,
    pub dur_ms: Option<u64>,

    // optional extension maps (observability metadata)
    pub attrs: Option<serde_json::Value>,
    pub meta: Option<serde_json::Value>,

    // payload (often present; not required by all consumers)
    pub data: Option<serde_json::Value>,
}
fn schema_event_v03() -> &'static str { EVENT_SCHEMA_V03 }

// ---------------- Term record (v0.1) ----------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TermRecordV01 {
    #[serde(default = "schema_term_record")]
    pub schema: &'static str,
    pub run_id: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub started_at: String,   // RFC3339
    pub finished_at: String,  // RFC3339
    pub exit_code: i32,
    pub dur_ms: u64,
    pub stdout_sha256: Option<String>,
    pub stderr_sha256: Option<String>,
}
fn schema_term_record() -> &'static str { TERM_RECORD_SCHEMA_V01 }

// ---------------- File watch event (v0.1) ----------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op")]
pub enum FileWatchEventV01 {
    #[serde(rename = "create")]
    Create {
        #[serde(default = "schema_file_watch")]
        schema: &'static str,
        ts: String,
        path: String,
        size: Option<u64>,
        sha256: Option<String>,
    },
    #[serde(rename = "modify")]
    Modify {
        #[serde(default = "schema_file_watch")]
        schema: &'static str,
        ts: String,
        path: String,
        size: Option<u64>,
        sha256: Option<String>,
    },
    #[serde(rename = "delete")]
    Delete {
        #[serde(default = "schema_file_watch")]
        schema: &'static str,
        ts: String,
        path: String,
    },
    #[serde(rename = "rename")]
    Rename {
        #[serde(default = "schema_file_watch")]
        schema: &'static str,
        ts: String,
        from: String,
        to: String,
    },
}
fn schema_file_watch() -> &'static str { FILE_WATCH_EVENT_SCHEMA_V01 }

// ---------------- Bag manifest (v0.2) ----------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BagManifestV02 {
    #[serde(default = "schema_bag_manifest")]
    pub schema: &'static str,

    pub version: String,              // "0.2"
    pub created_at: String,           // RFC3339
    pub format: String,               // "dir" | "tgz"

    pub counts: BagCounts,
    pub time_range: Option<BagTimeRange>,

    // topics map (typed for common topics; open to additional topics via additionalProperties)
    pub topics: Option<BagTopics>,

    pub chain_ok: bool,

    pub run_manifest_path: Option<String>,
    pub notes: Option<String>,

    // optional summaries included by replay tooling
    pub plugins: Option<BTreeMap<String, u64>>,
    pub commands: Option<BTreeMap<String, u64>>,
}
fn schema_bag_manifest() -> &'static str { BAG_MANIFEST_SCHEMA_V02 }

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BagCounts {
    pub events: u64,
    pub term_commands: u64,
    pub files_watch: u64,
    pub cline_tasks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BagTimeRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TopicRef {
    pub path: String,
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TermTopics {
    pub commands: Option<TopicRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BagTopics {
    pub events: Option<TopicRef>,
    pub term: Option<TermTopics>,
    pub files_watch: Option<TopicRef>,

    // Allow additional future topics
    #[serde(flatten)]
    pub other: Option<BTreeMap<String, serde_json::Value>>,
}
