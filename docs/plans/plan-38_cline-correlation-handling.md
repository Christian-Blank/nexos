Below is Implementation Plan #38 — “Cline correlation hardening”
(closes the loop for Human‑AI experiments; makes Cline ↔ runs/bags correlation first‑class, durable, and analyzable)

⸻

Goals
	1.	Deterministic correlation: For every run_id (e.g., a term.run) we can tell which Cline tasks were active around that run with a documented, repeatable rule (time‑window intersection + pad).
	2.	Durable records: Persist correlation as files under .nexoscli/cline/corr/ so it survives sessions and can be inspected, diffed, and bundled.
	3.	Bag enrichment: replay extract --include cline uses correlation to copy the matching Cline tasks into the bag and lists them in the bag manifest.
	4.	Doctor coverage: dev doctor --ci reports correlation coverage for recent runs to reveal blind spots (e.g., no Cline storage mounted).
	5.	No cross‑plugin deps: Keep plugins independent (file‑level integration only).

This closes the loop for Human‑in‑the‑Loop experiments: when you run commands locally (or with agents) and you chat/work in Cline, your bag contains both the CLI run and the contemporaneous Cline task(s), enabling prompt‑effectiveness analysis and end‑to‑end replay.

⸻

UX summary (new/updated commands)

# 1) Import tasks as before (already implemented)
nexoscli cline locate
nexoscli cline import --since 7d

# 2) Build correlation records (dry-run or persist)
nexoscli cline correlate --since 24h --pad 5m --dry-run
nexoscli cline correlate --run-id RUN-... --pad 5m --write

# 3) Extract a bag enriched with Cline tasks (reads correlation if present;
#    otherwise computes it on the fly with the same rule)
nexoscli replay extract --run-id RUN-... --include cline --out bag/

# 4) CI/Doctor shows correlation coverage for last 24h (info by default)
nexoscli dev doctor --ci --checks cline


⸻

Patch A — Cline “correlate” command (new module)

New file: plugins/cline/src/correlation.rs

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}, io::BufRead};
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
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
        let (created, updated) = match (created, updated) {
            (Some(a), Some(b)) => (a, b),
            (Some(a), None)    => (a, a),
            (None, Some(b))    => (b, b),
            (None, None)       => continue,
        };
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

fn persist_record(dot: &Path, rec: &CorrRecord) -> anyhow::Result<()> {
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

Wire it in: plugins/cline/src/lib.rs
	•	Add mod correlation; near top.
	•	In augment_commands() add:

.subcommand(
  ClapCommand::new("correlate").about("Correlate Cline tasks to runs")
    .arg(Arg::new("run-id").long("run-id").value_name("RUN"))
    .arg(Arg::new("since").long("since").value_name("WINDOW")) // 24h|7d|RFC3339
    .arg(Arg::new("pad").long("pad").value_name("DUR").default_value("5m")) // 5m|300s|1h
    .arg(Arg::new("write").long("write").action(ArgAction::SetTrue))
    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
)

	•	In execute() add handler:

Some(("correlate", m)) => {
    let dot = std::env::current_dir()?.join(".nexoscli");
    let pad = correlation::parse_pad(m.get_one::<String>("pad"));
    if let Some(run) = m.get_one::<String>("run-id") {
        let rec = correlation::correlate_run(&dot, run, pad)?;
        if m.get_flag("write") { correlation::persist_record(&dot, &rec)?; }
        return Ok(ctx.ok("cline","correlate.one",&serde_json::to_value(rec)?));
    } else {
        let since = correlation::parse_since(m.get_one::<String>("since"));
        let write = m.get_flag("write");
        let (sum, recs) = correlation::correlate_since(&dot, since, pad, write)?;
        return Ok(ctx.ok("cline","correlate",&serde_json::json!({
            "summary": sum, "count": recs.len(), "examples": recs.iter().take(2).collect::<Vec<_>>()
        })));
    }
}

	•	Optionally expose tool specs:

ToolSpec {
  name: "cline.correlate".into(),
  about: "Correlate Cline tasks to runs by time".into(),
  args_schema: schema_for!(serde_json::Value),
  result_schema: schema_for!(serde_json::Value),
},


⸻

Patch B — Replay extract: include Cline tasks using correlation

No cross‑crate dependency: we implement a tiny, local file‑based correlator inside the replay plugin that (1) reads .nexoscli/cline/corr/<run_id>.json if present, else (2) computes correlation by scanning .nexoscli/cline/tasks/ with the same rule (±pad; default 5m).

Add (or extend) in plugins/replay/src/extract.rs (or the file that implements extract):

use serde_json::Value;
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
use std::{fs, path::{Path, PathBuf}};
use walkdir::WalkDir;

// call this from the --include flow after base bag is built
pub fn include_cline(bag_dir: &Path, run_id: &str, pad: Duration) -> anyhow::Result<Vec<Value>> {
    let repo_dot = std::env::current_dir()?.join(".nexoscli");
    let corr_file = repo_dot.join("cline").join("corr").join(format!("{}.json", run_id));

    let (start, end) = compute_bag_window(bag_dir)?;
    let tasks = if corr_file.exists() {
        // use persisted correlation
        let txt = fs::read_to_string(&corr_file)?;
        let v: Value = serde_json::from_str(&txt)?;
        v.get("tasks").and_then(|x| x.as_array()).cloned().unwrap_or_default()
    } else {
        // scan tasks dir
        let tasks_dir = repo_dot.join("cline").join("tasks");
        scan_cline_tasks(&tasks_dir, start - pad, end + pad)?
    };

    // copy into bag & update manifest
    let mut listed = Vec::<Value>::new();
    let dst_dir = bag_dir.join("cline").join("tasks");
    fs::create_dir_all(&dst_dir)?;
    for t in &tasks {
        let id = t.get("id").and_then(|x| x.as_str())
            .or_else(|| t.get("path").and_then(|x| x.as_str()).and_then(|p| Path::new(p).file_stem().and_then(|s| s.to_str())))
            .unwrap_or("task");
        let src = t.get("path").and_then(|x| x.as_str()).map(PathBuf::from);
        let src = match src { Some(p) => p, None => continue };
        if !src.exists() { continue; }
        let created = t.get("created_at").and_then(|x| x.as_str())
            .map(|s| s.replace(':', "-")); // safe filename
        let fname = created.map(|c| format!("{}-{}.json", c, id)).unwrap_or_else(|| format!("{}.json", id));
        let dst = dst_dir.join(&fname);
        fs::copy(&src, &dst)?;
        let meta = fs::metadata(&dst)?;
        let sha = sha256(&fs::read(&dst)?);
        listed.push(serde_json::json!({
            "id": id,
            "dst": format!("cline/tasks/{}", fname),
            "sha256": sha,
            "bytes": meta.len(),
            "created_at": t.get("created_at").and_then(|x| x.as_str()),
            "messages": t.get("messages").and_then(|x| x.as_u64()).unwrap_or(0),
        }));
    }

    // augment manifest
    let man_p = bag_dir.join("manifest.json");
    let mut man_v: Value = serde_json::from_slice(&fs::read(&man_p)?)?;
    if let Some(obj) = man_v.as_object_mut() {
        obj.insert("cline_tasks".into(), Value::Array(listed.clone()));
        // maintain counts
        if let Some(c) = obj.get_mut("counts") {
            if let Some(map) = c.as_object_mut() {
                let n = listed.len() as u64;
                map.insert("cline".into(), Value::from(n));
            }
        }
    }
    fs::write(&man_p, serde_json::to_vec_pretty(&man_v)?)?;
    Ok(listed)
}

fn compute_bag_window(bag_dir: &Path) -> anyhow::Result<(OffsetDateTime, OffsetDateTime)> {
    // Derive from events.ndjson (single-run bag)
    let p = bag_dir.join("events.ndjson");
    let f = fs::File::open(&p)?;
    let br = std::io::BufReader::new(f);
    let mut start: Option<OffsetDateTime> = None;
    let mut end: Option<OffsetDateTime> = None;
    for line in br.lines().flatten() {
        if line.trim().is_empty() { continue; }
        let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        if let Some(ts) = v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()) {
            if start.map(|x| ts < x).unwrap_or(true) { start = Some(ts); }
            let dur = v.get("dur_ms").and_then(|x| x.as_i64()).unwrap_or(0);
            let local_end = ts + Duration::milliseconds(dur.max(0));
            if end.map(|x| local_end > x).unwrap_or(true) { end = Some(local_end); }
        }
    }
    let s = start.ok_or_else(|| anyhow::anyhow!("no events in bag"))?;
    let e = end.unwrap_or(s);
    Ok((s, e))
}

fn scan_cline_tasks(tasks_dir: &Path, start: OffsetDateTime, end: OffsetDateTime) -> anyhow::Result<Vec<Value>> {
    if !tasks_dir.exists() { return Ok(vec![]); }
    let mut out = vec![];
    for ent in WalkDir::new(tasks_dir).min_depth(1).max_depth(1).into_iter().flatten() {
        let p = ent.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
        let raw = fs::read(p)?;
        let v: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
        let created = v.get("createdAt").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
            .or_else(|| fs::metadata(p).ok().and_then(|m| m.modified().ok()).map(OffsetDateTime::from));
        let updated = v.get("updatedAt").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()).or(created);
        if let (Some(c), Some(u)) = (created, updated) {
            let ov = interval_overlap(c, u, start, end);
            if ov > 0 {
                out.push(serde_json::json!({
                    "id": p.file_stem().and_then(|s| s.to_str()).unwrap_or("task"),
                    "path": p.display().to_string(),
                    "created_at": c.format(&Rfc3339).unwrap(),
                    "updated_at": u.format(&Rfc3339).unwrap(),
                    "messages": v.get("messages").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0),
                }));
            }
        }
    }
    Ok(out)
}

fn interval_overlap(a0: OffsetDateTime, a1: OffsetDateTime, b0: OffsetDateTime, b1: OffsetDateTime) -> i64 {
    let s = std::cmp::max(a0, b0);
    let e = std::cmp::min(a1, b1);
    (e - s).whole_seconds().max(0)
}

fn sha256(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(b))
}

Integrate in plugins/replay/src/lib.rs (extract branch)

Inside your existing replay extract handler, after you’ve created the bag and written events.ndjson / manifest.json, add:

// honor --include cline (default pad = 5m)
if includes.contains(&"cline".to_string()) {
    let pad = time::Duration::minutes(5);
    let _list = crate::extract::include_cline(&bag_out_dir, run_id_str, pad)?;
    // history log entry if you maintain one
}

This keeps replay independent of cline at compile‑time. It only reads .nexoscli/cline/tasks and (optionally) .nexoscli/cline/corr/<run_id>.json if present.

⸻

Patch C — Doctor: show correlation coverage (info)

Edit plugins/dev/src/lib.rs in the --ci branch, cline check section:

Replace the existing cline block with:

// cline (informational unless required)
if want.contains("cline") {
    let mut ok = true;
    let mut detail = String::new();
    let mut fix = None;

    // locate (as before)
    let (c_loc, v_loc) = run_cli_json(&cwd, &["cline","locate"])?;
    if c_loc != 0 {
        ok = false;
        detail = "cline locate failed".into();
    } else {
        let sel = v_loc["data"]["selected"].as_str();
        if sel.is_none() {
            ok = false;
            detail = "no Cline storage found".into();
            fix = Some("Mount host globalStorage or set NEXOS_CLINE_STORAGE to /.../globalStorage/<extId>/tasks".into());
        } else {
            // dry-run import tiny sample
            let storage = sel.unwrap();
            let (_c_imp, _v_imp) = run_cli_json(&cwd, &["cline","import","--storage",storage,"--since","7d","--limit","5","--dry-run"])?;

            // correlation coverage for last 24h
            let (_c_cor, v_cor) = run_cli_json(&cwd, &["cline","correlate","--since","24h","--pad","5m","--dry-run"])?;
            let cov = v_cor["data"]["summary"]["coverage"].as_f64().unwrap_or(0.0);
            let total = v_cor["data"]["summary"]["runs_total"].as_u64().unwrap_or(0);
            detail = format!("coverage={:.0}% over {} runs", cov*100.0, total);
        }
    }

    checks.push(DoctorCiCheck {
        name: "cline".into(),
        ok,
        detail,
        severity: "info".into(),   // soft by default
        fix,
    });
}

This surfaces whether your experiment surface is actually producing correlatable data.

⸻

Patch D — E2E test

New: nexoscli-bin/tests/cline_correlation.rs

mod helpers; use helpers::*;
use std::{fs, path::PathBuf};
use serde_json::json;

#[test]
fn cline_correlation_and_bag_include() {
    let t = tmp();

    // 1) Set up fake Cline storage in test root
    let root = t.path().to_path_buf();
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", root.join("cline-global").display().to_string());
    let tasks_dir = root.join("cline-global").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();

    // Create a tiny task file with timestamps around "now"
    let now = chrono::Utc::now().to_rfc3339();
    let task = json!({
        "id":"task_1",
        "title":"demo",
        "createdAt": now,
        "updatedAt": now,
        "messages":[{"role":"user","content":"hi","createdAt":now}]
    });
    fs::write(tasks_dir.join("task_1.json"), serde_json::to_vec_pretty(&task).unwrap()).unwrap();

    // 2) init and import cline
    run_json(t.path(), &["dev","init","--yes"]);
    run_json(t.path(), &["cline","import","--since","7d"]);

    // 3) produce a term.run near "now"
    run_json(t.path(), &["term","run","--","bash","-lc","echo hi > a && git init && git config user.email t@e && git config user.name T && git add a && git commit -m init"]);

    // Last run_id
    let events = fs::read_to_string(t.path().join(".nexoscli/log/events.jsonl")).unwrap();
    let run_id = events.lines().rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v.get("plugin").and_then(|x| x.as_str())==Some("term"))
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .expect("run found");

    // 4) correlate & persist
    run_json(t.path(), &["cline","correlate","--run-id",&run_id,"--pad","5m","--write"]);
    assert!(t.path().join(".nexoscli/cline/corr").join(format!("{}.json", run_id)).exists());

    // 5) extract bag with cline included
    let bag = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id",&run_id,"--include","cline","--out", bag.to_str().unwrap()]);
    let cline_tasks = bag.join("cline/tasks");
    assert!(cline_tasks.exists());
    assert!(fs::read_to_string(bag.join("manifest.json")).unwrap().contains("cline_tasks"));
}


⸻

Patch E — Docs

Update docs/TRACEPACK.md, add a new section:

### Cline correlation (v0)

NEXOS correlates imported Cline tasks to CLI runs by **time‑window intersection** with a configurable pad (default **±5 minutes**).

**Build correlation**
```bash
# For a single run (persist to .nexoscli/cline/corr/RUN.json)
nexoscli cline correlate --run-id RUN-... --pad 5m --write

# For recent runs (dry-run, summary of coverage)
nexoscli cline correlate --since 24h --pad 5m --dry-run

Extract with Cline tasks

# Adds tasks into bag/cline/tasks and lists them in manifest.cline_tasks
nexoscli replay extract --run-id RUN-... --include cline --out bag/

Correlation record (example)

{
  "run_id": "RUN-...",
  "window": { "start": "...", "end": "...", "pad_sec": 300 },
  "event": { "ts":"...", "dur_ms": 4300, "plugin":"term","command":"run","exit_code":0 },
  "tasks": [
    { "id":"task_1", "path":".nexoscli/cline/tasks/task_1.json",
      "sha256":"sha256:...", "bytes":1234,
      "created_at":"...", "updated_at":"...", "messages": 12,
      "overlap_sec": 42, "pct_task_in_window": 0.67 }
  ],
  "created_at": "..."
}

**Update** `docs/PLAYBOOK.md` under Doctor/Health:

- Add: *“Cline correlation coverage (info): percentage of recent runs with at least one correlated Cline task. Use `cline import` and ensure storage is mounted in Dev Containers.”*

---

## Why this is the right v0 hardening

- **Deterministic rule** (time window + pad) → repeatable and explainable correlation; no heuristics magic.  
- **Durable artifacts** in `.nexoscli/cline/corr/` + **bag‑level inclusion** → experiments are portable and auditable.  
- **Doctor visibility** → you’ll know early if you’re missing Cline data (common in devcontainers/CI).  
- **Clean boundaries** → replay doesn’t import cline at build time; only via files (respects “no cross‑plugin deps” rule).  

---

## Suggested commit message

feat(cline+replay+dev): Cline correlation hardening (Plan #38)
	•	cline correlate: build durable correlation records per run (±pad)
	•	replay extract –include cline: copy correlated tasks into bag + manifest.cline_tasks
	•	dev doctor: report Cline correlation coverage (info) for last 24h
	•	docs: TRACEPACK.md — Cline correlation; PLAYBOOK doctor notes
	•	tests: cline_correlation_and_bag_include e2e

---

### Follow‑ups (post‑#38, small bites) - add to ROADMAP_NEXT.md as suggestions

- **Message‑level overlap** (count messages inside window) to sharpen overlap metrics.  
- **CSV export** for cline tasks per bag (`replay export --what cline`) to simplify pandas joins.  
- **Configurable correlation** (`.nexoscli/config.toml`) for pad default & plugin/command set to correlate against.  
- **UI quicklook** (later) once Parquet arrives.