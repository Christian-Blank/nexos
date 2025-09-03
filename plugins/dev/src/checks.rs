use serde::Serialize;
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};

pub fn redaction_ok() -> bool {
    let p = std::path::Path::new(".nexoscli").join("redact.yaml");
    let s = match std::fs::read_to_string(&p) { Ok(x) => x, Err(_) => return false };
    serde_yaml::from_str::<serde_json::Value>(&s).is_ok()
}

#[derive(Debug, Serialize)]
pub struct EventsWindow {
    pub ok: bool,
    pub total_lines: u64,
    pub with_task: u64,
    #[serde(skip_serializing_if="Option::is_none")]
    pub window_start: Option<String>,
}

pub fn events_window(since: &str) -> anyhow::Result<EventsWindow> {
    // Parse window
    let since_ts: Option<OffsetDateTime> = if since.ends_with('h') {
        since[..since.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h))
    } else if since.ends_with('d') {
        since[..since.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d))
    } else {
        OffsetDateTime::parse(since, &Rfc3339).ok()
    };

    let log = std::path::Path::new(".nexoscli").join("log").join("events.jsonl");
    if !log.exists() {
        return Ok(EventsWindow { ok: true, total_lines: 0, with_task: 0,
            window_start: since_ts.map(|t| t.format(&Rfc3339).unwrap()) });
    }

    let mut total: u64 = 0;
    let mut with_task: u64 = 0;
    let txt = std::fs::read_to_string(&log).unwrap_or_default();
    for line in txt.lines() {
        // Ignore malformed lines
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => { total += 1; continue; }
        };

        // Time filter (tolerant: include if no parseable ts)
        let in_window = match (since_ts, v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())) {
            (None, _)        => true,
            (Some(_), None)  => true,
            (Some(s), Some(t)) => t >= s,
        };
        if !in_window { continue; }

        total += 1;

        // Count if we have context.task_id
        if v.get("context")
            .and_then(|c| c.get("task_id"))
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .is_some()
        {
            with_task += 1;
        }
    }

    Ok(EventsWindow {
        ok: true,
        total_lines: total,
        with_task,
        window_start: since_ts.map(|t| t.format(&Rfc3339).unwrap()),
    })
}

// --- BEGIN: events checks (append) ---
use std::{fs, io::BufRead, path::PathBuf};
use sha2::{Digest, Sha256};
use serde::Deserialize;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DevCheckOutcome {
    pub name: String,
    pub ok: bool,
    pub lines: u64,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub digest: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EventsChain {
    schema: String,    // "events_chain.v0.1"
    file: String,
    from_ts: Option<String>,
    to_ts: Option<String>,
    lines: u64,
    sha256: String,
    generated_at: String,
}
const EVENTS_CHAIN_SCHEMA: &str = "events_chain.v0.1";

#[derive(Deserialize)]
struct EventLineV03Owned {
    schema: Option<String>,
    ts: Option<String>,
    run_id: Option<String>,
    plugin: Option<String>,
    command: Option<String>,
    exit_code: Option<i32>,
}

fn events_file(root: &std::path::Path) -> PathBuf {
    root.join(".nexoscli").join("log").join("events.jsonl")
}
fn chain_file(root: &std::path::Path) -> PathBuf {
    root.join(".nexoscli").join("log").join("events.chain.json")
}

pub fn check_events_schema(root: &std::path::Path, since: Option<OffsetDateTime>) -> DevCheckOutcome {
    let mut out = DevCheckOutcome{ name:"events-schema".into(), ok:true, lines:0, errors: vec![], digest: None, file: None };
    let path = events_file(root);
    out.file = Some(path.display().to_string());
    if !path.exists() {
        out.ok = true; // empty is ok
        return out;
    }
    let f = match fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => { out.ok=false; out.errors.push(format!("open: {}", e)); return out; }
    };
    let rdr = std::io::BufReader::new(f);
    for (idx, line) in rdr.lines().enumerate() {
        let raw = match line { Ok(s) => s, Err(e) => { out.ok=false; out.errors.push(format!("read line {}: {}", idx+1, e)); break; } };
        if raw.trim().is_empty() { continue; }

        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) => {
                // since filter (inclusive; tolerant on ts parse)
                if let Some(since_ts) = since {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                        if let Ok(t) = OffsetDateTime::parse(ts, &Rfc3339) {
                            if t < since_ts { continue; }
                        }
                    }
                }

                // decode against owned mirror type (avoids lifetime issues) and check schema tag
                match serde_json::from_str::<EventLineV03Owned>(&raw) {
                    Ok(_ev) => {
                        let tag_ok = v.get("schema").and_then(|x| x.as_str()) == Some(nexoscli_core::contracts::EVENT_SCHEMA_V03);
                        if !tag_ok {
                            out.ok = false;
                            out.errors.push(format!("line {}: missing or wrong schema tag", idx+1));
                        }
                    }
                    Err(e) => {
                        out.ok = false;
                        out.errors.push(format!("line {}: Event decode: {}", idx+1, e));
                    }
                }
                out.lines += 1;
            }
            Err(e) => { out.ok=false; out.errors.push(format!("line {}: json parse: {}", idx+1, e)); }
        }
    }
    out
}

#[derive(Copy, Clone)]
pub enum ChainMode { Verify, Refresh }

pub fn check_events_chain(root: &std::path::Path, mode: ChainMode) -> DevCheckOutcome {
    let mut out = DevCheckOutcome{ name:"events-chain".into(), ok:true, lines:0, errors: vec![], digest: None, file: None };
    let epath = events_file(root);
    out.file = Some(epath.display().to_string());
    if !epath.exists() { return out; }

    // rolling hash H = sha256(H || 0x0a || raw_line) with no leading separator before first line
    let f = match fs::File::open(&epath) {
        Ok(f) => f,
        Err(e) => { out.ok=false; out.errors.push(format!("open events: {}", e)); return out; }
    };
    let rdr = std::io::BufReader::new(f);
    let mut h = Sha256::new();
    let mut first = true;
    let mut from_ts: Option<String> = None;
    let mut to_ts: Option<String> = None;
    let mut lines = 0u64;

    for (idx, line) in rdr.lines().enumerate() {
        let raw = match line { Ok(s) => s, Err(e) => { out.ok=false; out.errors.push(format!("read line {}: {}", idx+1, e)); break; } };
        if raw.trim().is_empty() { continue; }
        if !first { h.update(&[0x0a]); }
        h.update(raw.as_bytes());
        first = false;

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if from_ts.is_none() {
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { from_ts = Some(ts.to_string()); }
            }
            if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { to_ts = Some(ts.to_string()); }
        }
        lines += 1;
    }
    let digest = format!("sha256:{:x}", h.finalize());
    out.digest = Some(digest.clone());
    out.lines = lines;

    let cpath = chain_file(root);
    match mode {
        ChainMode::Refresh => {
            // Always (over)write the chain file to reflect current events.jsonl state
            let rec = EventsChain{
                schema: EVENTS_CHAIN_SCHEMA.to_string(),
                file: epath.display().to_string(),
                from_ts,
                to_ts,
                lines,
                sha256: digest,
                generated_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
            };
            if let Some(dir) = cpath.parent() { let _ = fs::create_dir_all(dir); }
            if let Err(e) = fs::write(&cpath, serde_json::to_vec_pretty(&rec).unwrap()) {
                out.ok=false; out.errors.push(format!("write chain: {}", e));
            }
        }
        ChainMode::Verify => {
            if cpath.exists() {
                match fs::read_to_string(&cpath) {
                    Ok(txt) => {
                        match serde_json::from_str::<EventsChain>(&txt) {
                            Ok(rec) => {
                                if rec.sha256 != digest || rec.lines != lines {
                                    out.ok = false;
                                    out.errors.push("digest or line count mismatch; run `nexoscli dev doctor --checks events-chain` locally to refresh".into());
                                }
                            }
                            Err(e) => { out.ok=false; out.errors.push(format!("chain parse: {}", e)); }
                        }
                    }
                    Err(e) => { out.ok=false; out.errors.push(format!("chain read: {}", e)); }
                }
            } else {
                out.ok = false;
                out.errors.push("chain file missing; create with local doctor run".into());
            }
        }
    }

    out
}
// --- END: events checks ---
