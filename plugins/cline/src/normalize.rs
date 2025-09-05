use anyhow::Context;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io::Write, path::Path};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use walkdir::WalkDir;

/* ---- Provider-agnostic AI Message (AIM) row ---- */
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AimRow {
    pub stream: String,             // "Cline/<task_id>"
    pub provider: String,           // "cline"
    pub task_id: String,
    pub message_id: String,         // "cline:<task_id>:<idx>" if missing
    pub idx: u64,                   // index in original messages array
    pub role: String,               // user|assistant|system|tool
    pub created_at: String,         // RFC3339
    pub content_digest: String,     // sha256 of normalized text (or empty)
    #[serde(skip_serializing_if="Option::is_none")]
    pub text_preview: Option<String>, // first ~120 chars for human inspection
    pub code_blocks: Vec<CodeBlockDigest>,
    pub tool_calls: Vec<ToolCallDigest>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub tokens_out: Option<u64>,
    pub source_file: String,        // path to .nexoscli/cline/tasks/<id>.json
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodeBlockDigest { pub lang: String, pub lines: u64, pub digest: String }
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCallDigest { pub name: String, pub args_digest: String }

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct NormalizeSummary {
    pub scanned_tasks: u64,
    pub built_streams: u64,
    pub updated_streams: u64,
    pub skipped_unchanged: u64,
}

/* -------- Public entry points ---------- */

pub fn normalize_since(dot: &Path, since: Option<OffsetDateTime>, overwrite: bool, only_task: Option<&str>) -> anyhow::Result<NormalizeSummary> {
    let tasks_dir = dot.join("cline").join("tasks");
    fs::create_dir_all(&tasks_dir).ok();
    let mut sum = NormalizeSummary::default();

    for ent in WalkDir::new(&tasks_dir).min_depth(1).max_depth(1).into_iter().flatten() {
        let p = ent.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
        if let Some(only) = only_task {
            if p.file_stem().and_then(|s| s.to_str()) != Some(only) { continue; }
        }
        sum.scanned_tasks += 1;

        // Time filter (use file mtime since normalized files derive from this)
        if let Some(since_ts) = since {
            if let Ok(md) = fs::metadata(p) {
                if let Ok(m) = md.modified() {
                    if OffsetDateTime::from(m) < since_ts { continue; }
                }
            }
        }

        // Read normalized cline task
        let raw = fs::read(p)?;
        let v: Value = serde_json::from_slice(&raw).context("task JSON parse")?;

        // derive ids
        let task_id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("task").to_string();

        // Dest stream paths
        let stream_dir = dot.join("agents").join("streams").join("Cline").join(&task_id);
        let msgs_path  = stream_dir.join("messages.ndjson");
        let meta_path  = stream_dir.join("meta.json");

        // Skip if unchanged and not overwriting
        if !overwrite && msgs_path.exists() {
            // naive: if stream newer than task file → assume up-to-date
            let ok = fs::metadata(&msgs_path).and_then(|m| m.modified())
                .ok().map(|t| t > fs::metadata(p).and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)).unwrap_or(false);
            if ok { sum.skipped_unchanged += 1; continue; }
        }

        fs::create_dir_all(&stream_dir)?;

        // Build AIM rows
        let rows = extract_messages(&v, &task_id, p);

        // Write messages.ndjson (atomic-ish)
        let tmp = msgs_path.with_extension("ndjson.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            for r in &rows {
                let line = serde_json::to_string(r)?;
                writeln!(f, "{}", line)?;
            }
        }
        fs::rename(&tmp, &msgs_path)?;

        // Write meta
        let meta = serde_json::json!({
            "provider":"cline", "task_id": task_id, "stream": format!("Cline/{}", task_id),
            "source_file": p.display().to_string(),
            "messages": rows.len(),
            "updated_at": now()
        });
        fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;

        if overwrite || !msgs_path.exists() { sum.built_streams += 1; } else { sum.updated_streams += 1; }
    }

    Ok(sum)
}

/* --------- internals ---------- */

fn extract_messages(task: &Value, task_id: &str, source: &Path) -> Vec<AimRow> {
    let mut out = vec![];
    let msgs = task.get("messages").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let stream = format!("Cline/{}", task_id);
    let source_file = source.display().to_string();
    let fence = Regex::new(r"(?s)```([a-zA-Z0-9_\-\+\.]*)\n(.*?)```").unwrap();

    for (idx, m) in msgs.iter().enumerate() {
        let role = m.get("role").and_then(|x| x.as_str()).unwrap_or("assistant").to_string();
        // common fields across different Cline schema variants
        let created = m.get("createdAt").or_else(|| m.get("timestamp")).or_else(|| m.get("time"))
            .and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
            .unwrap_or_else(|| OffsetDateTime::now_utc());

        // content extraction (robust)
        let text = m.get("content").and_then(|x| x.as_str())
            .or_else(|| m.get("text").and_then(|x| x.as_str()))
            .unwrap_or("");
        let text_preview = if text.is_empty() { None } else { Some(crop(text, 120)) };
        let content_digest = sha256(text.as_bytes());

        // code fence extraction
        let mut code_blocks = vec![];
        for cap in fence.captures_iter(text) {
            let lang = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let body = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            code_blocks.push(CodeBlockDigest{
                lang,
                lines: body.lines().count() as u64,
                digest: sha256(body.as_bytes())
            });
        }

        // tool calls (if present)
        let mut tool_calls = vec![];
        if let Some(tc) = m.get("toolCalls").and_then(|x| x.as_array()) {
            for t in tc {
                let name = t.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let args = t.get("args").map(|a| serde_json::to_vec(a).unwrap_or_default()).unwrap_or_default();
                tool_calls.push(ToolCallDigest{ name, args_digest: sha256(&args) });
            }
        }

        // tokens (best effort; many logs won’t have them)
        let tokens_in = m.get("tokensIn").and_then(|x| x.as_u64());
        let tokens_out = m.get("tokensOut").and_then(|x| x.as_u64());

        // id
        let message_id = m.get("id").and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("cline:{}:{}", task_id, idx));

        out.push(AimRow{
            stream: stream.clone(),
            provider: "cline".into(),
            task_id: task_id.into(),
            message_id,
            idx: idx as u64,
            role,
            created_at: created.format(&Rfc3339).unwrap(),
            content_digest,
            text_preview,
            code_blocks,
            tool_calls,
            tokens_in, tokens_out,
            source_file: source_file.clone(),
        });
    }

    out
}

fn crop(s: &str, n: usize) -> String {
    let mut out = s.chars().take(n).collect::<String>();
    if s.chars().count() > n { out.push('…'); }
    out
}

fn sha256(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(b))
}

fn now() -> String { OffsetDateTime::now_utc().format(&Rfc3339).unwrap() }
