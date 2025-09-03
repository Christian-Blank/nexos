Below is Implementation Plan #36 — “Export helpers (CSV)”.
It adds a lightweight, dependency‑free (beyond csv crate) way to export events and fileops from a bag into analyst‑friendly CSV files. This is deliberately minimal and robust so we can use it in notebooks and spreadsheets immediately, and extend later (Parquet/annotations in #37).

⸻

What you get
	•	New command:
nexoscli replay export --bag <DIR> [--what events|fileops|all] [--as csv] [--out <PATH>]
	•	Defaults:
	•	--what: all
	•	--as: csv
	•	--out: <bag>/exports/csv/ (directory). Produces:
	•	<bag>/exports/csv/events.csv
	•	<bag>/exports/csv/fileops.csv
	•	Stable CSV schemas
events.csv (1 row per event)

ts,run_id,plugin,command,exit_code,dur_ms,error_code,args_hash,content_hash,task_id,stage,session_id,attrs_cmd,attrs_cwd,attrs_json

fileops.csv (1 row per file operation)

ts,run_id,op,path,from,to,bytes,digest_after,patch_sha256,patch_lines,source

attrs_json keeps the full attrs map as a JSON string for anything we don’t flatten into dedicated columns.

	•	Streaming, resilient export: reads NDJSON line‑by‑line, tolerates missing fields, and always writes headers.
	•	Tool spec: replay.export exposed for agents.
	•	Docs & E2E test added.

⸻

Patch 1 — Add dependency (plugins/replay/Cargo.toml)

[dependencies]
# ...existing...
csv = "1.3"


⸻

Patch 2 — Add exporter module

File: plugins/replay/src/export.rs (new)

use anyhow::Context;
use csv::WriterBuilder;
use serde_json::Value;
use std::{fs, io::BufRead, path::{Path, PathBuf}};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug)]
pub struct ExportCounts { pub events_rows: usize, pub fileops_rows: usize, pub skipped: usize }

fn as_str_opt(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string())
}
fn as_u64_opt(v: &Value, k: &str) -> Option<u64> {
    v.get(k).and_then(|x| x.as_u64())
}
fn ts_from(v: &Value) -> String {
    // Prefer "ts", else "timestamp", else now()
    as_str_opt(v, "ts")
        .or_else(|| as_str_opt(v, "timestamp"))
        .unwrap_or_else(|| OffsetDateTime::now_utc().format(&Rfc3339).unwrap())
}

pub fn export_events_csv(bag: &Path, out_path: &Path) -> anyhow::Result<usize> {
    let src = bag.join("events.ndjson");
    let f = fs::File::open(&src).with_context(|| format!("missing {}", src.display()))?;
    if let Some(parent) = out_path.parent() { fs::create_dir_all(parent)?; }
    let mut w = WriterBuilder::new().from_path(out_path)?;

    // header
    w.write_record([
        "ts","run_id","plugin","command","exit_code","dur_ms","error_code",
        "args_hash","content_hash","task_id","stage","session_id",
        "attrs_cmd","attrs_cwd","attrs_json"
    ])?;

    let br = std::io::BufReader::new(f);
    let mut rows = 0usize;
    for line in br.lines() {
        let line = match line { Ok(s) if !s.trim().is_empty() => s, _ => continue };
        let v: Value = match serde_json::from_str(&line) { Ok(v) => v, Err(_) => continue };

        let ts         = ts_from(&v);
        let run_id     = as_str_opt(&v, "run_id").unwrap_or_default();
        let plugin     = as_str_opt(&v, "plugin").unwrap_or_default();
        let command    = as_str_opt(&v, "command").unwrap_or_default();
        let exit_code  = as_u64_opt(&v, "exit_code").unwrap_or_default().to_string();
        let dur_ms     = as_u64_opt(&v, "dur_ms").unwrap_or_default().to_string();
        let error_code = as_str_opt(&v, "error_code").unwrap_or_default();
        let args_hash  = as_str_opt(&v, "args_hash").unwrap_or_default();
        let content_hash = as_str_opt(&v, "content_hash").unwrap_or_default();

        // context (task_id, stage, session_id) — tolerant across shapes
        let (task_id, stage, session_id) = {
            // v.context.* OR v.meta.context.*
            let ctx = v.get("context").cloned()
                .or_else(|| v.get("meta").and_then(|m| m.get("context")).cloned())
                .unwrap_or(Value::Null);
            let tid = ctx.get("task_id").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            let stg = ctx.get("stage").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            let ses = ctx.get("session_id").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            (tid, stg, ses)
        };

        // attrs flatten
        let attrs = v.get("attrs").cloned().unwrap_or(Value::Null);
        let attrs_cmd = attrs.get("cmd").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let attrs_cwd = attrs.get("cwd").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let attrs_json = if attrs.is_null() { "".to_string() } else { attrs.to_string() };

        w.write_record([
            ts, run_id, plugin, command, exit_code, dur_ms, error_code,
            args_hash, content_hash, task_id, stage, session_id,
            attrs_cmd, attrs_cwd, attrs_json
        ])?;
        rows += 1;
    }
    w.flush()?;
    Ok(rows)
}

pub fn export_fileops_csv(bag: &Path, out_path: &Path) -> anyhow::Result<usize> {
    let src = bag.join("fileops.ndjson");
    let f = fs::File::open(&src).with_context(|| format!("missing {}", src.display()))?;
    if let Some(parent) = out_path.parent() { fs::create_dir_all(parent)?; }
    let mut w = WriterBuilder::new().from_path(out_path)?;

    // header
    w.write_record([
        "ts","run_id","op","path","from","to","bytes","digest_after","patch_sha256","patch_lines","source"
    ])?;

    let br = std::io::BufReader::new(f);
    let mut rows = 0usize;
    for line in br.lines() {
        let line = match line { Ok(s) if !s.trim().is_empty() => s, _ => continue };
        let v: Value = match serde_json::from_str(&line) { Ok(v) => v, Err(_) => continue };

        let ts         = ts_from(&v);
        let run_id     = as_str_opt(&v, "run_id").unwrap_or_default();
        let op         = as_str_opt(&v, "op").unwrap_or_default();
        let path       = as_str_opt(&v, "path").unwrap_or_default();
        let from       = as_str_opt(&v, "from").unwrap_or_default();
        let to         = as_str_opt(&v, "to").unwrap_or_default();
        let bytes      = v.get("bytes").and_then(|x| x.as_u64()).map(|n| n.to_string()).unwrap_or_default();
        let digest_after = as_str_opt(&v, "digest_after").unwrap_or_default();

        // patch meta
        let (patch_sha, patch_lines) = v.get("patch").map(|p| {
            let s = p.get("sha256").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            let l = p.get("lines").and_then(|x| x.as_u64()).map(|n| n.to_string()).unwrap_or_default();
            (s, l)
        }).unwrap_or((String::new(), String::new()));

        let source = as_str_opt(&v, "source").unwrap_or_else(|| "patch".into());

        w.write_record([
            ts, run_id, op, path, from, to, bytes, digest_after, patch_sha, patch_lines, source
        ])?;
        rows += 1;
    }
    w.flush()?;
    Ok(rows)
}

/// Export selection; returns written paths + counts.
pub fn export_csv(bag: &Path, what: &str, out: Option<&Path>) -> anyhow::Result<(Vec<PathBuf>, ExportCounts)> {
    let out_dir = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| bag.join("exports").join("csv"));

    let mut written = Vec::<PathBuf>::new();
    let mut counts = ExportCounts { events_rows: 0, fileops_rows: 0, skipped: 0 };

    fs::create_dir_all(&out_dir)?;

    let want_events  = what == "all" || what.contains("events");
    let want_fileops = what == "all" || what.contains("fileops");

    if want_events {
        let dst = out_dir.join("events.csv");
        match export_events_csv(bag, &dst) {
            Ok(n) => { written.push(dst); counts.events_rows = n; }
            Err(_e) => { counts.skipped += 1; }
        }
    }
    if want_fileops {
        let dst = out_dir.join("fileops.csv");
        match export_fileops_csv(bag, &dst) {
            Ok(n) => { written.push(dst); counts.fileops_rows = n; }
            Err(_e) => { counts.skipped += 1; }
        }
    }

    Ok((written, counts))
}


⸻

Patch 3 — Wire into replay CLI

Open plugins/replay/src/lib.rs:
	1.	Imports / mods

mod fileops;   // from #35
mod schemas;   // from #34
mod export;    // NEW

	2.	Add subcommand under replay in augment_commands():

.subcommand(
  ClapCommand::new("export").about("Export bag data for analysis")
    .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
    .arg(Arg::new("what").long("what").value_name("events|fileops|all").default_value("all"))
    .arg(Arg::new("as").long("as").value_name("csv").default_value("csv"))
    .arg(Arg::new("out").long("out").value_name("PATH").help("Output dir (default: <bag>/exports/csv)"))
)

	3.	Handle execution in execute():

Some(("export", em)) => {
    let bag = em.get_one::<String>("bag").unwrap();
    let what = em.get_one::<String>("what").map(|s| s.as_str()).unwrap_or("all");
    let fmt  = em.get_one::<String>("as").map(|s| s.as_str()).unwrap_or("csv");
    let outp = em.get_one::<String>("out").cloned();

    let bagp = std::path::PathBuf::from(bag);
    if !bagp.is_dir() {
        return Ok(ctx.err("replay","export","replay.export.nodir","bag path is not a directory",None,2));
    }
    if fmt != "csv" {
        return Ok(ctx.err("replay","export","replay.export.badfmt","only --as csv is supported in v0",None,2));
    }

    let out_dir = outp.map(std::path::PathBuf::from);
    let (written, counts) = crate::export::export_csv(&bagp, what, out_dir.as_deref())?;
    let exit = ctx.ok("replay","export", &serde_json::json!({
        "bag": bag,
        "format": "csv",
        "written": written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "counts": { "events": counts.events_rows, "fileops": counts.fileops_rows, "skipped": counts.skipped }
    }));
    Ok(exit)
}

	4.	Tool spec (optional) in tool_specs():

ToolSpec{
  name: "replay.export".into(),
  about: "Export bag contents to analyst-friendly formats (v0: CSV)".into(),
  args_schema: schema_for!(serde_json::Value),
  result_schema: schema_for!(serde_json::Value),
},


⸻

Patch 4 — E2E test

nexoscli-bin/tests/replay_export_csv.rs (new)

mod helpers; use helpers::*;
use std::fs;

#[test]
fn replay_export_csv_events_and_fileops() {
    let t = tmp();

    // Prepare a repo and create a run that changes files
    run_json(t.path(), &["dev","init","--yes"]);
    run_json(t.path(), &["term","run","--","bash","-lc","git init && git config user.email t@e && git config user.name T && echo base> a.txt && git add a.txt && git commit -m init"]);
    run_json(t.path(), &["term","run","--","bash","-lc","echo x>> a.txt && echo y> b.txt && git add -A && git commit -m change && git mv b.txt c.txt || true"]);

    // Find last term.run run_id
    let events = fs::read_to_string(t.path().join(".nexoscli/log/events.jsonl")).unwrap();
    let last_run = events.lines().rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v.get("plugin").and_then(|x| x.as_str())==Some("term") && v.get("command").and_then(|x| x.as_str())==Some("run"))
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .expect("term.run event present");

    // Extract bag (this also derives fileops from patches via #35 integration)
    let bag = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id",&last_run,"--out",bag.to_str().unwrap()]);
    assert!(bag.join("events.ndjson").exists());
    assert!(bag.join("fileops.ndjson").exists());

    // Export to CSV (both)
    let v = run_json(t.path(), &["replay","export","--bag",bag.to_str().unwrap(),"--what","all","--as","csv"]);
    let written = v["data"]["written"].as_array().unwrap();
    assert!(written.iter().any(|x| x.as_str().unwrap().ends_with("events.csv")));
    assert!(written.iter().any(|x| x.as_str().unwrap().ends_with("fileops.csv")));

    let events_csv = bag.join("exports").join("csv").join("events.csv");
    let fileops_csv = bag.join("exports").join("csv").join("fileops.csv");
    let e_body = fs::read_to_string(&events_csv).unwrap();
    let f_body = fs::read_to_string(&fileops_csv).unwrap();
    assert!(e_body.lines().count() >= 2);   // header + at least one
    assert!(f_body.lines().count() >= 2);   // header + at least one

    // Sanity: header contains expected columns
    assert!(e_body.lines().next().unwrap().contains("plugin,command"));
    assert!(f_body.lines().next().unwrap().contains("op,path,from,to"));
}


⸻

Patch 5 — Docs

Append to docs/TRACEPACK.md under “Analyze”:

### CSV Export (analyst-friendly)

Convert a bag to CSV:

```bash
nexoscli replay export --bag ./tracepack-dir                # exports both events.csv and fileops.csv
nexoscli replay export --bag ./tracepack-dir --what events  # only events.csv
nexoscli replay export --bag ./tracepack-dir --what fileops # only fileops.csv

Default output directory: ./tracepack-dir/exports/csv/.

Columns
	•	events.csv: ts, run_id, plugin, command, exit_code, dur_ms, error_code, args_hash, content_hash, task_id, stage, session_id, attrs_cmd, attrs_cwd, attrs_json
	•	fileops.csv: ts, run_id, op, path, from, to, bytes, digest_after, patch_sha256, patch_lines, source

Load with Python:

import pandas as pd
events  = pd.read_csv('bag/exports/csv/events.csv', parse_dates=['ts'])
fileops = pd.read_csv('bag/exports/csv/fileops.csv', parse_dates=['ts'])

---

## Usage (quick smoke)

```bash
# Build
cargo build --workspace

# Make a run
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli term run -- bash -lc 'git init && git config user.email t@e && git config user.name T && echo hi> a && git add a && git commit -m init'
./target/debug/nexoscli term run -- bash -lc 'echo more>> a && echo x> b && git add -A && git commit -m chg && git mv b c || true'

# Extract + derive fileops (from #35 integration)
RUN=$(tail -n 1 .nexoscli/log/events.jsonl | jq -r '.run_id')
./target/debug/nexoscli replay extract --run-id "$RUN" --out bag

# Export CSV
./target/debug/nexoscli replay export --bag bag
ls bag/exports/csv
# events.csv  fileops.csv


⸻

Why this is the right v0
	•	Zero friction for analysts (CSV everywhere).
	•	Deterministic & streaming (no big memory loads, robust to odd lines).
	•	Schema‑aware enough to give useful columns but still preserve raw attrs as JSON.

⸻

Next step (#37 — Annotate & Export)

With export plumbing in place, we can implement annotations + richer exports:
	•	replay annotate (write/read annotations.ndjson in the bag; per‑event labels/reasons)
	•	replay export --as parquet (optional; Arrow/Parquet if we decide it’s worth the weight)
	•	replay export --join events+fileops (prejoined CSV keyed by ts/run_id)
	•	replay quicklook (rollup stats to screen: events by plugin, fileops by op/path)
