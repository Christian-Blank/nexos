use serde_json::json;

#[derive(Clone, Debug)]
pub struct SchemaInfo {
    pub name: &'static str,      // "events" | "fileops"
    pub version: &'static str,   // "0.3" | "0.1"
    pub id: &'static str,        // "nexos/events@0.3"
    pub description: &'static str,
}

pub fn list() -> Vec<SchemaInfo> {
    vec![
        SchemaInfo{
            name: "events",
            version: "0.3",
            id: "nexos/events@0.3",
            description: "Event lines (events.ndjson); RFC3339 ts; stable hashing fields; tolerant to extra fields.",
        },
        SchemaInfo{
            name: "fileops",
            version: "0.1",
            id: "nexos/fileops@0.1",
            description: "Repository file operations (fileops.ndjson); op/path/patch/digest_after (reserved v0.1).",
        },
    ]
}

pub fn get_schema_json(name: &str, version: &str) -> Option<serde_json::Value> {
    match (name, version) {
        ("events", "0.3") => Some(events_03()),
        ("fileops", "0.1") => Some(fileops_01()),
        _ => None,
    }
}

/* ---------- concrete schemas (Draft-07) ---------- */

fn events_03() -> serde_json::Value {
    // Make schema compatible with current core emission (event.v0.3) and
    // forward-compatible with normalized "nexos/events@0.3" lines.
    json!({
      "$schema": "http://json-schema.org/draft-07/schema#",
      "$id": "https://schemas.nexos.dev/nexos/events/0.3",
      "type": "object",
      "additionalProperties": true,
      "required": ["ts","run_id"],
      "properties": {
        "schema": { "type": "string", "enum": ["event.v0.3","nexos/events@0.3"] },
        "ts": { "type": "string" },
        "run_id": { "type": "string", "minLength": 1 },
        "plugin": { "type": "string", "minLength": 1 },
        "command": { "type": "string", "minLength": 1 },
        "exit_code": { "type": "integer" },
        "ok": { "type": "boolean" }, // optional for now; will be made required in follow-up
        "dur_ms": { "type": "integer", "minimum": 0 },
        "args_hash": { "type": "string" },
        "seq": { "type": "integer", "minimum": 1 },
        "prev": { "type": "string" },
        "content_hash": { "type": "string" },
        "chain_hash": { "type": "string" },
        "error_code": { "type": "string" },
        "data": { "type": ["object","array","string","number","boolean","null"] },
        "meta": { "type": "object", "additionalProperties": true },
        "context": {
          "type": "object",
          "additionalProperties": true,
          "properties": {
            "task_id": { "type": "string" },
            "session_id": { "type": "string" },
            "stage": { "type": "string" }
          }
        },
        "attrs": {
          "type": "object",
          "additionalProperties": true
        }
      }
    })
}

fn fileops_01() -> serde_json::Value {
    json!({
      "$schema": "http://json-schema.org/draft-07/schema#",
      "$id": "https://schemas.nexos.dev/nexos/fileops/0.1",
      "type": "object",
      "additionalProperties": true,
      "required": ["op","path"],
      "properties": {
        "schema": { "type": "string", "const": "nexos/fileops@0.1" },
        "ts": { "type": "string" },
        "run_id": { "type": "string" },
        "op": { "type": "string", "enum": ["create","modify","delete","rename"] },
        "path": { "type": "string", "minLength": 1 },
        "bytes": { "type": "integer", "minimum": 0 },
        "digest_after": { "type": "string", "pattern": "^sha256:[0-9a-fA-F]{64}$" },
        "patch": {
          "type": "object",
          "additionalProperties": true,
          "required": ["sha256"],
          "properties": {
            "sha256": { "type": "string", "pattern": "^sha256:[0-9a-fA-F]{64}$" },
            "lines": { "type": "integer", "minimum": 0 }
          }
        },
        "source": { "type": "string" },
        "context": {
          "type": "object",
          "additionalProperties": true,
          "properties": {
            "task_id": { "type": "string" },
            "session_id": { "type": "string" },
            "stage": { "type": "string" }
          }
        }
      }
    })
}
