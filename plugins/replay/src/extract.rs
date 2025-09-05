use serde_json::Value;
use std::{fs, path::Path};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use walkdir::WalkDir;

/// Include Cline AIM messages (AIM streams) that overlap the bag/run window.
/// Window resolution:
/// - Prefer staged events.ndjson in the bag staging directory (compute_bag_window)
/// - Fallback to deriving the run window directly from .nexoscli/log (compute_run_window_from_logs)
/// Tasks considered:
/// - Prefer persisted correlation record .nexoscli/cline/corr/<run_id>.json
/// - Fallback to all streams under .nexoscli/agents/streams/Cline/*
/// Writes cline/messages.ndjson into the staged bag directory and returns number of lines written.
pub fn include_cline_messages(stage_dir: &Path, run_id: &str, pad: Duration) -> anyhow::Result<u64> {
    // Compute window (with padding applied)
    let (start, end) = match compute_bag_window(stage_dir) {
        Ok((win_start, win_end)) => (win_start - pad, win_end + pad),
        Err(_) => compute_run_window_from_logs(run_id, pad)?,
    };

    // Repo dot
    let repo_dot = std::env::current_dir()?.join(".nexoscli");

    // Determine tasks for this run: prefer correlation record
    let corr_file = repo_dot.join("cline").join("corr").join(format!("{}.json", run_id));
    let task_ids: Vec<String> = {
        let mut ids: Vec<String> = Vec::new();
        if corr_file.exists() {
            if let Ok(raw) = fs::read(&corr_file) {
                if let Ok(v) = serde_json::from_slice::<Value>(&raw) {
                    ids = v.get("tasks")
                        .and_then(|x| x.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|t| t.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()))
                                .collect::<Vec<String>>()
                        })
                        .unwrap_or_default();
                }
            }
        }
        if ids.is_empty() {
            // Fallback: all Cline streams present
            ids = streams_for_provider(&repo_dot, "Cline");
        }
        ids
    };

    // Filter messages by window and collect lines
    let mut kept: u64 = 0;
    let mut out_lines: Vec<String> = Vec::new();
    for tid in &task_ids {
        let msgs_path = repo_dot
            .join("agents")
            .join("streams")
            .join("Cline")
            .join(&tid)
            .join("messages.ndjson");
        if !msgs_path.exists() {
            continue;
        }
        let s = fs::read_to_string(&msgs_path).unwrap_or_default();
        for line in s.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let ts = v
                .get("created_at")
                .and_then(|x| x.as_str())
                .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
            if let Some(ts) = ts {
                if ts >= start && ts <= end {
                    out_lines.push(line.to_string());
                    kept += 1;
                }
            }
        }
    }

    // If no messages overlapped (e.g., extremely short runs without events window), include all messages for selected tasks as a fallback
    if kept == 0 {
        // Recompute without time filter over ALL Cline streams to ensure deterministic bag enrichment
        let all_ids = streams_for_provider(&repo_dot, "Cline");
        for tid in all_ids {
            let msgs_path = repo_dot
                .join("agents")
                .join("streams")
                .join("Cline")
                .join(&tid)
                .join("messages.ndjson");
            if !msgs_path.exists() { continue; }
            let s = fs::read_to_string(&msgs_path).unwrap_or_default();
            for line in s.lines() {
                if line.trim().is_empty() { continue; }
                out_lines.push(line.to_string());
                kept += 1;
            }
        }
    }

    if !out_lines.is_empty() {
        let dst = stage_dir.join("cline").join("messages.ndjson");
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dst, out_lines.join("\n").as_bytes())?;
    }
    Ok(kept)
}

/// Derive the staged bag window from events.ndjson inside the staging directory.
fn compute_bag_window(stage_dir: &Path) -> anyhow::Result<(OffsetDateTime, OffsetDateTime)> {
    let evp = stage_dir.join("events.ndjson");
    let txt = fs::read_to_string(&evp)
        .map_err(|e| anyhow::anyhow!("read events.ndjson from stage: {}", e))?;
    let mut first: Option<OffsetDateTime> = None;
    let mut last: Option<OffsetDateTime> = None;
    for line in txt.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if let Some(ts) = v
                .get("ts")
                .and_then(|x| x.as_str())
                .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
            {
                if first.is_none() {
                    first = Some(ts);
                }
                last = Some(ts);
            }
        }
    }
    match (first, last) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err(anyhow::anyhow!("no timestamps found in staged events.ndjson")),
    }
}

/// Fallback: derive window directly from .nexoscli/log for a run when staged events.ndjson is not present.
/// Returns (start,end) with padding already applied.
fn compute_run_window_from_logs(run_id: &str, pad: Duration) -> anyhow::Result<(OffsetDateTime, OffsetDateTime)> {
    let root = std::env::current_dir()?;
    let log_dir = root.join(".nexoscli").join("log");

    // Prefer per-run file
    let run_file = log_dir.join("runs").join(format!("{}.jsonl", run_id));
    let mut first: Option<OffsetDateTime> = None;
    let mut last: Option<OffsetDateTime> = None;

    let mut consider_line = |line: &str| {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if v.get("run_id").and_then(|x| x.as_str()) == Some(run_id) {
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()) {
                    if first.is_none() { first = Some(ts); }
                    last = Some(ts);
                }
            }
        }
    };

    if run_file.exists() {
        if let Ok(txt) = fs::read_to_string(&run_file) {
            for l in txt.lines() { consider_line(l); }
        }
    } else if log_dir.exists() {
        for ent in walkdir::WalkDir::new(&log_dir).min_depth(1).max_depth(1).into_iter().flatten() {
            let p = ent.path();
            if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
            if let Ok(txt) = fs::read_to_string(p) {
                for l in txt.lines() { consider_line(l); }
            }
        }
    }

    match (first, last) {
        (Some(f), Some(l)) => Ok((f - pad, l + pad)),
        _ => Err(anyhow::anyhow!("could not determine window for run {}", run_id)),
    }
}

fn streams_for_provider(dot: &Path, provider: &str) -> Vec<String> {
    let root = dot.join("agents").join("streams").join(provider);
    if !root.exists() {
        return vec![];
    }
    let mut ids = vec![];
    for ent in WalkDir::new(&root)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .flatten()
    {
        if ent.file_type().is_dir() {
            if let Some(id) = ent.file_name().to_str() {
                ids.push(id.to_string());
            }
        }
    }
    ids
}
