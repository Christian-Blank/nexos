Awesome—let’s knock out the next two Temporal‑readiness items we agreed on:
	1.	Session correlation (session_id) end‑to‑end
	2.	Doctor checks (redaction present & valid, event integrity, task‑coverage)

Everything below is drop‑in. It preserves your current behavior and adds the next layer of safety. I’m keeping changes small and testable.

⸻

1) Session correlation (add session_id everywhere)

Goal: a stable ID that ties work across CLI runs, Cline tails, and bundles. It’s created when a task starts/resumes and injected into every event/envelope while that task is active.

1.1 Extend the task ledger (sessions carry an id)

plugins/task/src/lib.rs – change the Session struct and add a generator:

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Session { 
  id: String,            // NEW: stable identifier for the session
  start: String, 
  end: Option<String> 
}

fn gen_session_id() -> String {
  use sha2::{Digest, Sha256};
  let seed = format!("{}-{}", time::OffsetDateTime::now_utc().unix_timestamp_nanos(), std::process::id());
  let d = Sha256::digest(seed.as_bytes());
  format!("SES-{}", &hex::encode(d)[..12])
}

Add hex = "0.4" to the workspace dependencies (root Cargo.toml), if you don’t already have it:

[workspace.dependencies]
hex = "0.4"

1.2 Persist the “current session” marker when starting/resuming

In task start and task resume handlers, when you push a session, assign an id and write a small marker file:

// start
let sid = gen_session_id();
rec.sessions.push(Session{ id: sid.clone(), start: now(), end: None });
rec.save()?;
set_current(&rec.id)?;
set_current_session(&sid)?;      // NEW

// resume
let sid = gen_session_id();
rec.sessions.push(Session{ id: sid.clone(), start: now(), end: None });
rec.save()?;
set_current(&rec.id)?;
set_current_session(&sid)?;      // NEW

On pause and end, clear the marker:

clear_current()?; 
clear_current_session()?;        // NEW

Add helpers near the other current helpers:

fn set_current_session(id: &str) -> anyhow::Result<()> {
  let p = PathBuf::from(".nexoscli").join("task");
  std::fs::create_dir_all(&p)?;
  std::fs::write(p.join("current_session"), id.as_bytes())?;
  Ok(())
}
fn clear_current_session() -> anyhow::Result<()> {
  let p = PathBuf::from(".nexoscli").join("task").join("current_session");
  let _ = std::fs::remove_file(p);
  Ok(())
}
fn current_session_id() -> Option<String> {
  std::fs::read_to_string(PathBuf::from(".nexoscli").join("task").join("current_session"))
    .ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

1.3 Inject session_id into every event & envelope

nexoscli-core/src/envelope.rs – extend active_task_map() to include session_id:

fn active_task_map(&self) -> Option<Map<String, Value>> {
    let cur = PathBuf::from(".nexoscli").join("task").join("current");
    let id = std::fs::read_to_string(&cur).ok()?.trim().to_string();
    if id.is_empty() { return None; }
    let ledger = PathBuf::from(".nexoscli").join("tasks").join(&id).join("task.json");
    let stage = std::fs::read_to_string(&ledger).ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("stage").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "implementation".to_string());
    // NEW: session_id
    let sid = std::fs::read_to_string(PathBuf::from(".nexoscli").join("task").join("current_session"))
        .ok().map(|s| s.trim().to_string());
    let mut m = Map::new();
    m.insert("task_id".into(), Value::String(id));
    m.insert("stage".into(), Value::String(stage));
    if let Some(sid) = sid { m.insert("session_id".into(), Value::String(sid)); }
    Some(m)
}

No other call sites change—your ok() / err() already include the context map in events and meta.

1.4 Include session_id in Task bundles

plugins/task/src/lib.rs – in bundle after computing first_ts/last_ts, add to the manifest:

let manifest = json!({
  "version":"0.1","kind":"tracepack.task",
  "task_id": id,
  "session_id": current_session_id(),            // NEW: best-effort
  "created_at": now(),
  "time_range": { "from": first_ts, "to": last_ts },
  "counts": { ... },
  "patches": manifest_patches,
  "status_files": manifest_status,
  "cline_tasks": manifest_cline
});

1.5 Test (E2E)

nexoscli-bin/tests/session_id.rs:

mod helpers; use helpers::*;
use std::{fs, process::Command};

#[test]
fn session_id_propagates_into_events_and_bundle() {
    let t = tmp();
    // task start creates session
    let j = run_json(t.path(), &["task","new","--title","Sess"]);
    let tid = j["data"]["id"].as_str().unwrap().to_string();
    run_ok(t.path(), &["task","start","--task",&tid]);

    // a term run emits an event with context.session_id
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();
    let v = run_json(t.path(), &["term","run","sh","-lc","echo y >> a.txt"]);
    assert!(v["meta"]["context"]["session_id"].is_string());

    // bundle includes session_id in manifest
    let out = t.path().join("packs").join("t1");
    run_ok(t.path(), &["task","bundle","--task",&tid,"--out", out.to_str().unwrap()]);
    let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    assert!(m.get("session_id").is_some());
}


⸻

2) dev doctor (checks: redaction, event integrity, task coverage)

Goal: one command that tells you “am I green to proceed?” and can fail CI if required checks aren’t met.

2.1 Add the doctor command (or extend it) in plugins/dev/src/lib.rs

Register:

.subcommand(
  ClapCommand::new("doctor").about("Environment & hygiene checks")
    .arg(Arg::new("since").long("since").default_value("24h"))
    .arg(Arg::new("require").long("require").help("CSV of checks to enforce").default_value(""))
    .arg(Arg::new("task_coverage_min").long("task-coverage-min").default_value("0.7"))
    .arg(Arg::new("output").long("output").value_parser(["text","json"]).default_value("json"))
)

Execute:

Some(("doctor", sm)) => {
    let since = sm.get_one::<String>("since").unwrap();
    let require = sm.get_one::<String>("require").unwrap().split(',').filter(|s|!s.is_empty()).collect::<Vec<_>>();
    let min_cov: f64 = sm.get_one::<String>("task_coverage_min").unwrap().parse().unwrap_or(0.7);
    let outfmt = sm.get_one::<String>("output").unwrap();

    let mut report = serde_json::Map::new();
    let mut hard_fail = false;

    // 1) Redaction config present & valid
    let red_ok = crate::checks::redaction_ok();
    report.insert("redaction".into(), serde_json::json!({ "ok": red_ok }));
    if !red_ok && require.iter().any(|c| *c == "redaction") { hard_fail = true; }

    // 2) Event integrity & counts for window
    let ev = crate::checks::events_window(since)?;
    report.insert("events".into(), serde_json::to_value(&ev)?);
    if (!ev.ok) && require.iter().any(|c| *c == "events") { hard_fail = true; }

    // 3) Task coverage
    let cov = if ev.total_lines > 0 { ev.with_task as f64 / ev.total_lines as f64 } else { 1.0 };
    let task_cov_ok = cov >= min_cov;
    report.insert("task_coverage".into(), serde_json::json!({ "coverage": cov, "min": min_cov, "ok": task_cov_ok }));
    if (!task_cov_ok) && require.iter().any(|c| *c == "tasks") { hard_fail = true; }

    let envelope = serde_json::json!({ "data": report, "errors": [], "meta": { "run_id": ctx.providers.run_id() } });
    let code = if hard_fail { ctx.err("dev","doctor","DOCTOR_FAILED","one or more required checks failed",None,2) } else { ctx.ok("dev","doctor",&envelope) };
    Ok(CommandOutput{ message: None, data: Some(envelope), exit_code: code })
}

Add mod checks; at top of the file and create plugins/dev/src/checks.rs:

use serde::Serialize;
use std::{fs, path::PathBuf};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use walkdir::WalkDir;

pub fn redaction_ok() -> bool {
    let p = PathBuf::from(".nexoscli").join("redact.yaml");
    let s = match fs::read_to_string(&p) { Ok(x) => x, Err(_) => return false };
    serde_yaml::from_str::<serde_json::Value>(&s).is_ok()
}

#[derive(Debug, Serialize)]
pub struct EventsWindow {
    pub ok: bool,
    pub total_lines: u64,
    pub valid_lines: u64,
    pub with_task: u64,
    pub errors: u64,
    pub by_plugin: std::collections::BTreeMap<String,u64>,
}

pub fn events_window(since: &str) -> anyhow::Result<EventsWindow> {
    // parse "24h" / "2h" / "7d" (simple)
    let (num, unit) = since.split_at(since.len()-1);
    let n: i64 = num.parse().unwrap_or(24);
    let dur = match unit { "h" => Duration::hours(n), "d" => Duration::days(n), _ => Duration::hours(24) };
    let cutoff = OffsetDateTime::now_utc() - dur;

    let root = PathBuf::from(".nexoscli").join("events");
    let mut r = EventsWindow {
        ok: true, total_lines:0, valid_lines:0, with_task:0, errors:0, by_plugin: Default::default()
    };
    if !root.exists() { return Ok(r); }

    for ent in WalkDir::new(&root).min_depth(1).max_depth(1).into_iter().flatten() {
        let p = ent.path();
        if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
        let txt = fs::read_to_string(p).unwrap_or_default();
        for line in txt.lines() {
            if line.trim().is_empty() { continue; }
            r.total_lines += 1;
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(v) => {
                    let ts_ok = v.get("ts").and_then(|x| x.as_str())
                        .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
                        .map(|t| t >= cutoff).unwrap_or(false);
                    if !ts_ok { continue; } // outside window
                    r.valid_lines += 1;
                    if v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0) != 0 { r.errors += 1; }
                    if v.pointer("/context/task_id").is_some() { r.with_task += 1; }
                    if let Some(pl) = v.get("plugin").and_then(|x| x.as_str()) {
                        *r.by_plugin.entry(pl.to_string()).or_insert(0) += 1;
                    }
                }
                Err(_) => { r.ok = false; }
            }
        }
    }
    Ok(r)
}

This is intentionally “light”: it doesn’t need an events plugin; it just validates shape and counts within the window.

2.2 Test (E2E)

nexoscli-bin/tests/dev_doctor.rs:

mod helpers; use helpers::*;
use std::{fs, process::Command};

#[test]
fn doctor_green_with_redaction_and_task_coverage() {
    let t = tmp();

    // redaction config present
    let rd = t.path().join(".nexoscli"); std::fs::create_dir_all(&rd).unwrap();
    fs::write(rd.join("redact.yaml"), "version:0.1\npatterns: []\n").unwrap();

    // minimal repo & task & one event
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    let tid = run_json(t.path(), &["task","new","--title","Doc"]).get("data").unwrap().get("id").unwrap().as_str().unwrap().to_string();
    run_ok(t.path(), &["task","start","--task",&tid]);
    run_ok(t.path(), &["term","run","sh","-lc","echo y >> a.txt"]);

    let rep = run_json(t.path(), &["dev","doctor","--require","redaction,events,tasks","--since","24h"]);
    let data = rep["data"].clone();
    assert!(data.get("redaction").unwrap().get("ok").unwrap().as_bool().unwrap());
    assert!(data.get("events").unwrap().get("valid_lines").unwrap().as_u64().unwrap() >= 1);
    assert!(data.get("task_coverage").unwrap().get("ok").unwrap_or(&serde_json::Value::Bool(true)).as_bool().unwrap());
}


⸻

3) Docs (quick additions)
	•	docs/OBSERVABILITY.md — add a short “Sessions” paragraph:

### Sessions (`session_id`)
Starting or resuming a task creates a new `session_id` (e.g., `SES-…`) recorded in `.nexoscli/task/current_session`. While active, every CLI command’s event and JSON envelope includes `meta.context.session_id`. Task bundles copy this into `manifest.json`.

	•	docs/PLAYBOOK.md — new “Doctor” section:

## Health checks (doctor)
Run before starting a bigger effort or in CI:

```bash
nexoscli dev doctor --require redaction,events,tasks --since 24h --output json

	•	redaction: .nexoscli/redact.yaml exists and parses.
	•	events: event lines in the window are valid JSON with required fields.
	•	tasks: at least X% of events in the window are tagged with a task_id (default 70%; override with --task-coverage-min).

- **`docs/CLI_REFERENCE.md`** — regenerate (if you have a generator) or add entries for `dev doctor`.

---

## 4) What you can expect after merging

- Every event now carries `{task_id, stage, session_id}` when a task is active.  
- You can confidently ask “what happened in session X?” and extract a TaskPack with that context.  
- `nexoscli dev doctor` gives you a **single, enforceable** gate before/after work (and in CI) to catch the most common drift.

