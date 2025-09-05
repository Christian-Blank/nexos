---
status: done
plan_id: '39'
slug: agent-grade-cline-capture
task_id: TASK-20250905-import-cline-tasks-unchanged
started_at: 2025-09-05T19:42:18.268941Z
source: next
completed_at: 2025-09-05T20:25:28.920757Z
implemented_in: a34e74c
---
Below is Implementation Plan #39 — “Agent‑grade Cline capture (message‑level normalization & correlation)”
(hardens Human↔AI observability so experiments are analyzable like fully automated runs)

⸻

What this delivers

Outcome: after any run (e.g., term.run) you’ll have message‑level records of Cline prompts/responses normalized into a provider‑agnostic stream under .nexoscli/agents/, plus per‑run correlation (which messages overlapped the run window), bag enrichment (tasks and messages copied into the bag) and basic HAI metrics.

Concretely:
	•	Normalize → .nexoscli/agents/streams/Cline/<task_id>/messages.ndjson
Rows are provider‑agnostic “AI Message” (AIM) records (id, role, created_at, text digest, code block digests, etc.) produced from the already‑imported normalized Cline tasks under .nexoscli/cline/tasks/….
	•	Correlate messages to runs with the same time‑window ± pad rule used for tasks, and persist into .nexoscli/cline/corr/<run_id>.json (augmented with msg_stats).
	•	Bag enrichment: replay extract --include cline now adds:
	•	cline/tasks/*.json (already from #38), and
	•	cline/messages.ndjson (only overlapping messages), with manifest.counts.cline_messages.
	•	Doctor visibility: dev doctor --ci prints Cline correlation coverage and message coverage across last 24h (informational unless required).
	•	HAI telemetry: dev telemetry hai --since 24h aggregates agent‑level metrics: prompts/responses counts, per‑role distribution, median user→assistant latency, code‑block counts.

No cross‑crate deps: we only read/write files—each plugin remains independent.

⸻

CLI surfaces

# Import Cline tasks (unchanged)
nexoscli cline locate
nexoscli cline import --since 7d

# NEW: Normalize Cline tasks → provider‑agnostic AIM streams
nexoscli cline normalize --since 7d            # incremental; uses .nexoscli/cline/tasks/*
nexoscli cline normalize --task-id task_abc123 # precise
nexoscli cline normalize --overwrite           # force re-build

# Correlate (now also message-level stats)
nexoscli cline correlate --since 24h --pad 5m --dry-run
nexoscli cline correlate --run-id RUN-... --pad 5m --write

# Enriched bags (tasks + messages)
nexoscli replay extract --run-id RUN-... --include cline --out bag/

# Doctor shows Cline coverage (tasks + messages)
nexoscli dev doctor --ci --checks cline

# NEW: HAI telemetry (basic metrics over AIM streams)
nexoscli dev telemetry hai --since 24h


⸻

Patch A — Add normalization module (Cline → AIM)

File: plugins/cline/Cargo.toml (add regex)

[dependencies]
# ...existing
regex = { workspace = true }

New file: plugins/cline/src/normalize.rs

use anyhow::Context;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, io::Write, path::{Path, PathBuf}};
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
        let rows = extract_messages(&v, &task_id, p)?;

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


⸻

Patch B — Wire “normalize” and extend “correlate”

Edit: plugins/cline/src/lib.rs
	1.	Add at top:

mod correlation;   // from Plan #38 (already added)
mod normalize;     // NEW

	2.	augment_commands(): add a new subcommand and extend correlate flags:

.subcommand(
  ClapCommand::new("normalize").about("Normalize Cline tasks → AIM streams (.nexoscli/agents/streams/Cline/<task>/messages.ndjson)")
    .arg(Arg::new("since").long("since").value_name("WINDOW"))
    .arg(Arg::new("task-id").long("task-id").value_name("ID"))
    .arg(Arg::new("overwrite").long("overwrite").action(ArgAction::SetTrue))
)
.subcommand(
  ClapCommand::new("correlate").about("Correlate Cline tasks/messages to runs")
    .arg(Arg::new("run-id").long("run-id").value_name("RUN"))
    .arg(Arg::new("since").long("since").value_name("WINDOW"))
    .arg(Arg::new("pad").long("pad").value_name("DUR").default_value("5m"))
    .arg(Arg::new("write").long("write").action(ArgAction::SetTrue))
    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
)

	3.	execute(): add normalize handler; and enhance correlate output with message stats.

Some(("normalize", m)) => {
    let dot = std::env::current_dir()?.join(".nexoscli");
    let since = correlation::parse_since(m.get_one::<String>("since"));
    let overwrite = m.get_flag("overwrite");
    let only_task = m.get_one::<String>("task-id").map(|s| s.as_str());
    let sum = normalize::normalize_since(&dot, since, overwrite, only_task)?;
    return Ok(ctx.ok("cline","normalize",&serde_json::to_value(sum)?));
}

Some(("correlate", m)) => {
    let dot = std::env::current_dir()?.join(".nexoscli");
    let pad = correlation::parse_pad(m.get_one::<String>("pad"));
    // ensure streams exist (best-effort incremental build)
    let _ = normalize::normalize_since(&dot, None, false, None);

    if let Some(run) = m.get_one::<String>("run-id") {
        let mut rec = correlation::correlate_run(&dot, run, pad)?;
        // attach message stats for overlapping window
        let msg_stats = correlate_messages_for_run(&dot, &rec)?;
        // augment json
        let mut v = serde_json::to_value(&rec)?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert("msg_stats".into(), msg_stats);
        }
        if m.get_flag("write") {
            // persist original core record first
            correlation::persist_record(&dot, &rec)?;
            // and write message stats sibling
            let corr_dir = dot.join("cline").join("corr");
            fs::write(corr_dir.join(format!("{}.messages.json", rec.run_id)), serde_json::to_vec_pretty(&v)?)?;
        }
        return Ok(ctx.ok("cline","correlate.one",&v));
    } else {
        let since = correlation::parse_since(m.get_one::<String>("since"));
        let write = m.get_flag("write");
        let (sum, recs) = correlation::correlate_since(&dot, since, pad, write)?;
        // compute simple coverage for messages too
        let mut with_msgs = 0u64;
        for r in &recs {
            let stats = correlate_messages_for_run(&dot, r)?;
            if stats.get("messages_in_window").and_then(|x| x.as_u64()).unwrap_or(0) > 0 { with_msgs += 1; }
        }
        let msg_cov = if recs.is_empty() { 1.0 } else { with_msgs as f64 / recs.len() as f64 };
        return Ok(ctx.ok("cline","correlate",&serde_json::json!({
            "summary": sum, "count": recs.len(), "message_coverage": msg_cov
        })));
    }
}

	4.	Helper in plugins/cline/src/lib.rs (below execute()), to compute message stats:

use time::{OffsetDateTime, format_description::well_known::Rfc3339, Duration};

fn correlate_messages_for_run(dot: &std::path::Path, rec: &correlation::CorrRecord) -> serde_json::Value {
    use serde_json::json;
    // derive window
    let start = OffsetDateTime::parse(&rec.window.start, &Rfc3339).ok();
    let end   = OffsetDateTime::parse(&rec.window.end, &Rfc3339).ok();
    if start.is_none() || end.is_none() { return json!({}); }
    let (start, end) = (start.unwrap(), end.unwrap());

    // load AIM streams for listed tasks
    let mut total = 0u64;
    let mut by_role = std::collections::BTreeMap::<String,u64>::new();
    let mut user_ts: Option<OffsetDateTime> = None;
    let mut first_assistant_ts: Option<OffsetDateTime> = None;
    let mut code_blocks = 0u64;

    for t in &rec.tasks {
        let tid = t.id.clone();
        let msgs_path = dot.join("agents").join("streams").join("Cline").join(&tid).join("messages.ndjson");
        if !msgs_path.exists() { continue; }
        if let Ok(lines) = std::fs::read_to_string(&msgs_path) {
            for line in lines.lines() {
                let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, _ => continue };
                let ts = v.get("created_at").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
                if ts.is_none() { continue; }
                let ts = ts.unwrap();
                if ts < start || ts > end { continue; }
                total += 1;
                let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("assistant").to_string();
                *by_role.entry(role.clone()).or_insert(0) += 1;
                if role == "user" { user_ts = Some(ts); }
                if role == "assistant" && first_assistant_ts.is_none() { first_assistant_ts = Some(ts); }
                if let Some(arr) = v.get("code_blocks").and_then(|x| x.as_array()) { code_blocks += arr.len() as u64; }
            }
        }
    }

    // latency (ms): first assistant reply after latest preceding user msg in window
    let latency_ms = match (user_ts, first_assistant_ts) {
        (Some(u), Some(a)) if a >= u => (a - u).whole_milliseconds().max(0) as u64,
        _ => 0
    };

    serde_json::json!({
        "messages_in_window": total,
        "by_role": by_role,
        "first_reply_latency_ms": latency_ms,
        "code_blocks_in_window": code_blocks
    })
}


⸻

Patch C — Replay: include messages in bags

File: plugins/replay/src/extract.rs (add next to the include_cline from #38)

use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}};
use walkdir::WalkDir;

/* Include Cline AIM messages that overlap bag window (±pad applied at caller) */
pub fn include_cline_messages(bag_dir: &Path, run_id: &str, pad: Duration) -> anyhow::Result<u64> {
    let repo_dot = std::env::current_dir()?.join(".nexoscli");
    let corr_file = repo_dot.join("cline").join("corr").join(format!("{}.json", run_id));

    let (win_start, win_end) = compute_bag_window(bag_dir)?;
    let start = win_start - pad;
    let end   = win_end + pad;

    // Which tasks?
    let task_ids: Vec<String> = if corr_file.exists() {
        let v: Value = serde_json::from_slice(&fs::read(&corr_file)?)?;
        v.get("tasks").and_then(|x| x.as_array()).map(|a|
            a.iter().filter_map(|t| t.get("id").and_then(|x| x.as_str()).map(|s| s.to_string())).collect()
        ).unwrap_or_default()
    } else {
        // fallback to all streams
        streams_for_provider(&repo_dot, "Cline")
    };

    let mut kept = 0u64;
    let mut out_lines = Vec::<String>::new();
    for tid in task_ids {
        let p = repo_dot.join("agents").join("streams").join("Cline").join(&tid).join("messages.ndjson");
        if !p.exists() { continue; }
        let s = fs::read_to_string(&p)?;
        for line in s.lines() {
            let v: Value = match serde_json::from_str(line) { Ok(v) => v, _ => continue };
            let ts = v.get("created_at").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
            if let Some(ts) = ts {
                if ts >= start && ts <= end {
                    out_lines.push(line.to_string());
                    kept += 1;
                }
            }
        }
    }

    if kept > 0 {
        let dst = bag_dir.join("cline").join("messages.ndjson");
        if let Some(parent) = dst.parent() { fs::create_dir_all(parent)?; }
        fs::write(&dst, out_lines.join("\n").as_bytes())?;
        // bump manifest counts
        let man_p = bag_dir.join("manifest.json");
        let mut v: Value = serde_json::from_slice(&fs::read(&man_p)?)?;
        if let Some(obj) = v.as_object_mut() {
            if let Some(counts) = obj.get_mut("counts").and_then(|x| x.as_object_mut()) {
                counts.insert("cline_messages".into(), Value::from(kept));
            }
        }
        fs::write(&man_p, serde_json::to_vec_pretty(&v)?)?;
    }
    Ok(kept)
}

fn streams_for_provider(dot: &Path, provider: &str) -> Vec<String> {
    let root = dot.join("agents").join("streams").join(provider);
    if !root.exists() { return vec![]; }
    let mut ids = vec![];
    for ent in WalkDir::new(&root).min_depth(1).max_depth(1).into_iter().flatten() {
        if ent.file_type().is_dir() {
            if let Some(id) = ent.file_name().to_str() { ids.push(id.to_string()); }
        }
    }
    ids
}

File: plugins/replay/src/lib.rs (inside extract command, after writing events.ndjson + manifest.json and after include_cline(...) from #38)

if includes.contains(&"cline".to_string()) {
    let pad = time::Duration::minutes(5);
    let _ = crate::extract::include_cline(&bag_out_dir, run_id_str, pad)?;
    let _ = crate::extract::include_cline_messages(&bag_out_dir, run_id_str, pad)?;
}


⸻

Patch D — Doctor: message coverage

File: plugins/dev/src/lib.rs (in the existing --ci cline block you added for #38)

Add after computing coverage={:.0}%:

// message coverage (fraction of recent runs with ≥1 overlapping AIM message)
let (_c_cor2, v_cor2) = run_cli_json(&cwd, &["cline","correlate","--since","24h","--pad","5m","--dry-run"])?;
let msg_cov = v_cor2["data"]["message_coverage"].as_f64().unwrap_or(0.0);
detail = format!("task_cov={:.0}% msg_cov={:.0}% over {} runs", cov*100.0, msg_cov*100.0, total);


⸻

Patch E — HAI Telemetry

File: plugins/dev/src/lib.rs (augment commands)

Add a subcommand under dev telemetry:

.subcommand(
  Command::new("hai")
    .about("Aggregate AI-interaction metrics from AIM streams (.nexoscli/agents/streams/*)")
    .arg(Arg::new("since").long("since").value_name("WINDOW"))
)

In execute() under Some(("telemetry", tm)) add:

else if let Some(("hai", m)) = tm.subcommand() {
    let since_s = m.get_one::<String>("since").cloned();
    let since_ts = match since_s.as_deref() {
        None => None,
        Some(s) if s.ends_with('h') => Some(time::OffsetDateTime::now_utc() - time::Duration::hours(s[..s.len()-1].parse::<i64>().unwrap_or(24))),
        Some(s) if s.ends_with('d') => Some(time::OffsetDateTime::now_utc() - time::Duration::days(s[..s.len()-1].parse::<i64>().unwrap_or(7))),
        Some(s) => time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok(),
    };

    let dot = std::env::current_dir()?.join(".nexoscli").join("agents").join("streams");
    let mut prompts = 0u64;
    let mut responses = 0u64;
    let mut first_reply_latencies = Vec::<u64>::new();
    let mut code_blocks = 0u64;

    if dot.exists() {
        for prov in std::fs::read_dir(&dot)? {
            let prov = prov?;
            if !prov.file_type()?.is_dir() { continue; }
            for sid in std::fs::read_dir(prov.path())? {
                let sid = sid?;
                if !sid.file_type()?.is_dir() { continue; }
                let msgs = sid.path().join("messages.ndjson");
                if !msgs.exists() { continue; }
                let s = std::fs::read_to_string(&msgs)?;
                let mut last_user_ts: Option<time::OffsetDateTime> = None;
                let mut counted_reply = false;
                for line in s.lines() {
                    let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, _ => continue };
                    let ts = v.get("created_at").and_then(|x| x.as_str()).and_then(|s| time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok());
                    if let Some(since) = since_ts { if ts.map(|t| t < since).unwrap_or(false) { continue; } }
                    let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("assistant");
                    match role {
                        "user" => { prompts += 1; last_user_ts = ts; counted_reply = false; },
                        "assistant" => {
                            responses += 1;
                            if let (Some(u), Some(a)) = (last_user_ts, ts) {
                                if !counted_reply && a >= u { first_reply_latencies.push((a - u).whole_milliseconds().max(0) as u64); counted_reply = true; }
                            }
                            if let Some(arr) = v.get("code_blocks").and_then(|x| x.as_array()) { code_blocks += arr.len() as u64; }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    first_reply_latencies.sort_unstable();
    let p50 = percentile_u64(&first_reply_latencies, 50.0);
    let p90 = percentile_u64(&first_reply_latencies, 90.0);

    let out = serde_json::json!({
        "since": since_s,
        "prompts": prompts,
        "responses": responses,
        "reply_latency_ms": { "p50": p50, "p90": p90, "count": first_reply_latencies.len() },
        "code_blocks": code_blocks
    });
    let exit = ctx.ok("dev","telemetry.hai",&out);
    return Ok(exit);
}

Helper near bottom of file:

fn percentile_u64(sorted: &Vec<u64>, pct: f64) -> Option<u64> {
    if sorted.is_empty() { return None; }
    let mut v = sorted.clone();
    v.sort_unstable();
    let n = v.len() as f64;
    let idx = (pct * (n - 1.0) / 100.0).round() as usize;
    v.get(idx).copied()
}


⸻

Patch F — E2E test

File: nexoscli-bin/tests/agents_hai_e2e.rs

mod helpers; use helpers::*;
use std::{fs, path::PathBuf};
use serde_json::json;

#[test]
fn hai_normalize_correlate_and_bag() {
    let t = tmp();

    // Fake Cline storage
    let root = t.path().to_path_buf();
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", root.join("cline-global").display().to_string());
    let tasks_dir = root.join("cline-global").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();

    // Create a task with messages surrounding "now"
    let now = chrono::Utc::now();
    let u1 = now.to_rfc3339();
    let a1 = (now + chrono::Duration::milliseconds(800)).to_rfc3339();

    let task = json!({
        "id":"task_norm",
        "title":"demo",
        "createdAt": u1,
        "updatedAt": a1,
        "messages": [
            { "role":"user", "content":"Write a hello world in Rust\n```rust\nfn main() { println!(\"hi\"); }\n```", "createdAt": u1 },
            { "role":"assistant", "content":"Here you go:\n```rust\nfn main(){println!(\"hi\");}\n```", "createdAt": a1 }
        ]
    });
    fs::write(tasks_dir.join("task_norm.json"), serde_json::to_vec_pretty(&task).unwrap()).unwrap();

    // init + import + normalize
    run_json(t.path(), &["dev","init","--yes"]);
    run_json(t.path(), &["cline","import","--since","7d"]);
    run_json(t.path(), &["cline","normalize","--since","7d"]);

    // Check stream exists
    assert!(t.path().join(".nexoscli/agents/streams/Cline/task_norm/messages.ndjson").exists());

    // Create a run near "now"
    run_json(t.path(), &["term","run","--","bash","-lc","echo x > x.txt"]);

    // last run_id
    let evs = fs::read_to_string(t.path().join(".nexoscli/log/events.jsonl")).unwrap();
    let run_id = evs.lines().rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v.get("plugin").and_then(|x| x.as_str())==Some("term"))
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .unwrap();

    // correlate (persist)
    run_json(t.path(), &["cline","correlate","--run-id",&run_id,"--pad","5m","--write"]);
    assert!(t.path().join(".nexoscli/cline/corr").join(format!("{}.json", run_id)).exists());

    // extract bag
    let bag = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id",&run_id,"--include","cline","--out", bag.to_str().unwrap()]);

    // verify messages.ndjson in bag
    let msgs = bag.join("cline/messages.ndjson");
    assert!(msgs.exists());
    let body = fs::read_to_string(&msgs).unwrap();
    assert!(body.contains("\"role\":\"user\""));
    assert!(body.contains("\"role\":\"assistant\""));

    // telemetry hai
    let r = run_json(t.path(), &["dev","telemetry","hai","--since","7d"]);
    assert!(r["data"]["prompts"].as_u64().unwrap() >= 1);
    assert!(r["data"]["responses"].as_u64().unwrap() >= 1);
}


⸻

Docs

Update docs/TRACEPACK.md (new subsection)

### Agent Interaction Messages (AIM) — Cline (v0)

When `--include cline` is used, the extractor adds:

- `cline/tasks/…` — the normalized Cline task JSONs that overlapped the run window.
- `cline/messages.ndjson` — **provider‑agnostic** message rows (AIM) filtered to the run window ± pad.

AIM schema (per line):
```json
{
  "stream": "Cline/<task_id>",
  "provider": "cline",
  "task_id": "task_norm",
  "message_id": "cline:task_norm:0",
  "idx": 0,
  "role": "user",
  "created_at": "2025-09-03T12:00:00Z",
  "content_digest": "sha256:…",
  "text_preview": "Write a hello world …",
  "code_blocks": [{"lang":"rust","lines":1,"digest":"sha256:…"}],
  "tool_calls": [],
  "tokens_in": null, "tokens_out": null,
  "source_file": ".nexoscli/cline/tasks/task_norm.json"
}

Correlate messages → runs
	•	Build AIM streams: nexoscli cline normalize --since 7d
	•	Correlate: nexoscli cline correlate --run-id RUN-... --pad 5m --write

Analyze in Python (example)

import json, pandas as pd
rows = [json.loads(line) for line in open("bag/cline/messages.ndjson")]
df = pd.DataFrame(rows)
print(df.groupby("role").size())

**Update** `docs/OBSERVABILITY.md` (new section “AIM streams”);  
**Update** `docs/PLAYBOOK.md` (doctor notes → “Cline task/message coverage”).

---

## Why this is the right next step

- **No more blind spots**: prompts/responses are first‑class data, not just task blobs.  
- **Provider‑agnostic**: AIM rows let you later ingest other agents (local runner, Temporal workers) into the same streams folder without changing analytics.  
- **Portable**: Bags now contain the exact prompts/responses that influenced a run — critical for reproducible HAI experiments.  
- **Actionable metrics**: latency and by‑role counts show prompt‑engineering effects immediately; code‑block counts hint at code‑suggestion intensity.  

---

## Suggested commit

feat(cline+replay+dev): Plan #39 — Agent-grade Cline capture
	•	cline normalize: Cline → AIM streams under .nexoscli/agents/streams/Cline//messages.ndjson
	•	cline correlate: attach message-level stats; persist .messages.json (sibling)
	•	replay extract –include cline: add cline/messages.ndjson + counts.cline_messages
	•	dev telemetry hai: prompts/responses, reply latency p50/p90, code block counts
	•	dev doctor: Cline task_cov + msg_cov coverage (info)
	•	docs: TRACEPACK (AIM), PLAYBOOK doctor notes, OBSERVABILITY (AIM streams)
	•	e2e: agents_hai_e2e covers normalize→correlate→bag→telemetry

---

## Small follow‑ups (bite‑size) - add to ROADMAP_NEXT.md

1) **Message CSV export**: `replay export --what cline_messages --out messages.csv` (makes pandas even easier).  
2) **Token accounting**: add optional “tokenizer hints” (approx length) when tokens aren’t in source.  
3) **Per‑message ↔ activity linkage**: later, annotate AIM rows with nearest `term.run` id to accelerate joins.  
4) **Config**: default pads and providers under `[observability]` in `.nexoscli/config.toml`.

If you want, I can now prepare the exact insertion point in your `plugins/replay/src/lib.rs` handler (few lines) to call `include_cline_messages()` right after `include_cline()`.