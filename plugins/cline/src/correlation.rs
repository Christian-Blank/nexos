use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::BufRead,
    path::{Path, PathBuf},
};
use time::{
    format_description::well_known::Rfc3339, Duration, OffsetDateTime,
};
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CorrTask {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub messages: Option<u64>,
    pub overlap_sec: u64,       // approximate: interval overlap
    pub pct_task_in_window: f64 // overlap / task_span (0..1)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CorrRecord {
    pub run_id: String,
    pub window: Window,
    pub event: RunEvent,
    pub tasks: Vec<CorrTask>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Window { pub start: String, pub end: String, pub pad_sec: i64 }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunEvent {
    pub ts: String,
    pub dur_ms: u64,
    pub plugin: String,
    pub command: String,
    pub exit_code: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CorrSummary {
    pub runs_total: u64,
    pub runs_with_tasks: u64,
    pub runs_without_tasks: u64,
    pub coverage: f64, // with_tasks / total
    pub examples: Vec<String>, // a few run_ids
}

/* ---------- public API ---------- */

pub fn correlate_since(dot: &Path, since: Option<OffsetDateTime>, pad: Duration, write: bool) -> anyhow::Result<(CorrSummary, Vec<CorrRecord>)> {
    let runs = select_runs(dot, since)?;
    let mut summary = CorrSummary::default();
    let mut out = Vec::<CorrRecord>::new();
    for ev in runs {
        summary.runs_total += 1;
        let rec = correlate_run(dot, &ev.run_id, pad)?;
        if !rec.tasks.is_empty() { summary.runs_with_tasks += 1; } else { summary.runs_without_tasks += 1; }
        if out.len() < 3 { summary.examples.push(ev.run_id.clone()); }
        if write { persist_record(dot, &rec)?; }
        out.push(rec);
    }
    summary.coverage = if summary.runs_total > 0 { (summary.runs_with_tasks as f64) / (summary.runs_total as f64) } else { 1.0 };
    Ok((summary, out))
}

pub fn correlate_run(dot: &Path, run_id: &str, pad: Duration) -> anyhow::Result<CorrRecord> {
    let ev = find_run(dot, run_id)?.context("run not found")?;
    let (start, end) = event_window(&ev, pad);
    let tasks = scan_tasks(dot, start, end)?;
    Ok(CorrRecord {
        run_id: ev.run_id.clone(),
        window: Window { start: start.format(&Rfc3339).unwrap(), end: end.format(&Rfc3339).unwrap(), pad_sec: pad.whole_seconds() },
        event: RunEvent {
            ts: ev.ts.format(&Rfc3339).unwrap(),
            dur_ms: ev.dur_ms as u64,
            plugin: ev.plugin,
            command: ev.command,
            exit_code: ev.exit_code
        },
        tasks,
        created_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
    })
}

/* ---------- internals ---------- */

#[derive(Debug, Clone)]
struct Ev { run_id: String, ts: OffsetDateTime, dur_ms: i64, plugin: String, command: String, exit_code: i64 }

fn select_runs(dot: &Path, since: Option<OffsetDateTime>) -> anyhow::Result<Vec<Ev>> {
    let log = dot.join("log").join("events.jsonl");
    if !log.exists() { return Ok(vec![]); }
    let f = fs::File::open(&log)?;
    let br = std::io::BufReader::new(f);
    let mut out = Vec::<Ev>::new();
    for line in br.lines() {
        let s = match line { Ok(s) => s, _ => continue };
        if s.trim().is_empty() { continue; }
        let v: Value = match serde_json::from_str(&s) { Ok(v) => v, Err(_) => continue };
        let plugin = v.get("plugin").and_then(|x| x.as_str()).unwrap_or("");
        let command = v.get("command").and_then(|x| x.as_str()).unwrap_or("");
        // focus on term.run (extendable later)
        if !(plugin == "term" && command == "run") { continue; }
        let run_id = v.get("run_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let ts = v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
        if ts.is_none() { continue; }
        if let Some(since_ts) = since {
            if ts.unwrap() < since_ts { continue; }
        }
        let dur_ms = v.get("dur_ms").and_then(|x| x.as_i64()).unwrap_or(0);
        let exit = v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0);
        out.push(Ev{ run_id, ts: ts.unwrap(), dur_ms, plugin: plugin.to_string(), command: command.to_string(), exit_code: exit });
    }
    Ok(out)
}

fn find_run(dot: &Path, run_id: &str) -> anyhow::Result<Option<Ev>> {
    let log = dot.join("log").join("events.jsonl");
    if !log.exists() { return Ok(None); }
    let f = fs::File::open(&log)?;
    let br = std::io::BufReader::new(f);
    for line in br.lines().flatten() {
        if line.contains(run_id) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if v.get("run_id").and_then(|x| x.as_str()) == Some(run_id) {
                    let ts = v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
                    let dur_ms = v.get("dur_ms").and_then(|x| x.as_i64()).unwrap_or(0);
                    let plugin = v.get("plugin").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let command = v.get("command").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let exit = v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0);
                    if let Some(ts) = ts {
                        return Ok(Some(Ev{ run_id: run_id.into(), ts, dur_ms, plugin, command, exit_code: exit }));
                    }
                }
            }
        }
    }
    Ok(None)
}

fn event_window(ev: &Ev, pad: Duration) -> (OffsetDateTime, OffsetDateTime) {
    let start = ev.ts - pad;
    let end = ev.ts + Duration::milliseconds(ev.dur_ms.max(0)) + pad;
    (start, end)
}

fn parse_ts(v: &Value, key: &str) -> Option<OffsetDateTime> {
    v.get(key).and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
}

fn scan_tasks(dot: &Path, start: OffsetDateTime, end: OffsetDateTime) -> anyhow::Result<Vec<CorrTask>> {
    let tasks_dir = dot.join("cline").join("tasks");
    if !tasks_dir.exists() { return Ok(vec![]); }
    let mut out = Vec::<CorrTask>::new();
    for ent in WalkdirIter::new(&tasks_dir) {
        let p = ent?;
        if p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
        let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
        let raw = fs::read(&p)?;
        let v: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
        // prefer createdAt/updatedAt from normalized JSON; fallback to fs mtime
        let created = parse_ts(&v, "createdAt").or_else(|| metadata_ts(&p));
        let updated = parse_ts(&v, "updatedAt").or_else(|| created);
        let (mut created, mut updated) = match (created, updated) {
            (Some(a), Some(b)) => (a, b),
            (Some(a), None)    => (a, a),
            (None, Some(b))    => (b, b),
            (None, None)       => continue,
        };
        // Ensure non-zero span for mtime-only tasks so overlap registers when within window
        if updated <= created {
            updated = created + Duration::seconds(1);
        }
        // overlap?
        let overlap = interval_overlap(created, updated, start, end);
        if overlap <= 0 { continue; }

        let span = (updated - created).whole_seconds().max(1) as f64;
        let pct = (overlap as f64 / span).clamp(0.0, 1.0);
        let msgs = v.get("messages").and_then(|x| x.as_array()).map(|a| a.len() as u64);

        out.push(CorrTask {
            id: id.clone(),
            path: p.display().to_string(),
            sha256: sha256(&raw),
            bytes: raw.len() as u64,
            created_at: Some(created.format(&Rfc3339).unwrap()),
            updated_at: Some(updated.format(&Rfc3339).unwrap()),
            messages: msgs,
            overlap_sec: overlap as u64,
            pct_task_in_window: pct
        });
    }
    // sort by overlap desc
    out.sort_by(|a,b| b.overlap_sec.cmp(&a.overlap_sec));
    Ok(out)
}

fn interval_overlap(a0: OffsetDateTime, a1: OffsetDateTime, b0: OffsetDateTime, b1: OffsetDateTime) -> i64 {
    let s = std::cmp::max(a0, b0);
    let e = std::cmp::min(a1, b1);
    (e - s).whole_seconds().max(0)
}

fn metadata_ts(p: &Path) -> Option<OffsetDateTime> {
    fs::metadata(p).ok()?.modified().ok().map(OffsetDateTime::from)
}

pub fn persist_record(dot: &Path, rec: &CorrRecord) -> anyhow::Result<()> {
    let dir = dot.join("cline").join("corr");
    fs::create_dir_all(&dir)?;
    let dst = dir.join(format!("{}.json", rec.run_id));
    fs::write(&dst, serde_json::to_vec_pretty(rec)?)?;
    Ok(())
}

fn sha256(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(b))
}

struct WalkdirIter{ it: walkdir::IntoIter }
impl WalkdirIter {
    fn new(root: &Path) -> Self { Self{ it: WalkDir::new(root).min_depth(1).max_depth(1).into_iter() } }
}
impl Iterator for WalkdirIter {
    type Item = anyhow::Result<PathBuf>;
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(e) = self.it.next() {
            match e {
                Ok(de) if de.file_type().is_file() => return Some(Ok(de.path().to_path_buf())),
                Ok(_) => continue,
                Err(err) => return Some(Err(err.into()))
            }
        }
        None
    }
}

/* ---------- parsing helpers ---------- */

pub fn parse_since(s: Option<&String>) -> Option<OffsetDateTime> {
    match s {
        None => None,
        Some(s) if s.ends_with('h') => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
        Some(s) if s.ends_with('d') => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
        Some(s) => OffsetDateTime::parse(s, &Rfc3339).ok(),
    }
}
pub fn parse_pad(s: Option<&String>) -> Duration {
    match s {
        None => Duration::minutes(5),
        Some(s) if s.ends_with('s') => Duration::seconds(s[..s.len()-1].parse::<i64>().unwrap_or(300)),
        Some(s) if s.ends_with('m') => Duration::minutes(s[..s.len()-1].parse::<i64>().unwrap_or(5)),
        Some(s) if s.ends_with('h') => Duration::hours(s[..s.len()-1].parse::<i64>().unwrap_or(1)),
        Some(num) => Duration::seconds(num.parse::<i64>().unwrap_or(300)),
    }
}
