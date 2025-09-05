---
status: in-progress
plan_id: '37'
slug: annotate-and-export
task_id: TASK-20250905-add-to-an-event-row-by-sequence-index
started_at: 2025-09-05T09:49:16.949232Z
source: next
---
Below is Implementation Plan #37 — “Annotate & Export”.
It adds a minimal, durable annotations layer for bags and the ability to export annotations (CSV) and join them into your events/fileops CSVs for analysis.

⸻

What you get

New CLI (replay):
	•	replay annotate add — append an annotation to a bag (per‑event, per‑fileop, or bag‑level)
	•	replay annotate list — list annotations in a bag
	•	replay export (enhanced)
	•	--what annotations — export annotations.csv
	•	--join annotations — produce events_annotated.csv and/or fileops_annotated.csv by joining annotations (keyed by row sequence)

Bag files (v0):

<bag>/
  annotations.ndjson    # one JSON object per line (schema below)
  events.ndjson
  fileops.ndjson
  exports/csv/
    events.csv
    fileops.csv
    annotations.csv
    events_annotated.csv
    fileops_annotated.csv

Annotation record (stable v0 schema):

{
  "ts": "RFC3339 (when annotation created)",
  "source": "human|agent|rule|workflow",
  "author": "optional string",
  "labels": ["string", "..."],
  "note": "optional string",
  "confidence": 0.0,
  "target": {
    "kind": "event|fileop|bag",
    "seq": 12,                 // 0-based row index in events/fileops (required for event|fileop)
    "ts": "RFC3339",           // copied from target row if available (helps humans)
    "run_id": "RUN-...",       // copied from target row if available
    "plugin": "term",          // events only (best-effort)
    "command": "run",          // events only (best-effort)
    "op": "write|move|...",    // fileops only (best-effort)
    "path": "a.txt"            // fileops only (best-effort)
  },
  "attrs": { "free": "shape" } // optional structured extras
}

Design choice: target.seq is the v0 join key. It is stable for a given bag because events.csv/fileops.csv are written in the same order as their NDJSON sources.

⸻

Patch 1 — New module: plugins/replay/src/annotate.rs

// plugins/replay/src/annotate.rs
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, Write},
    path::Path,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Target {
    pub kind: String, // "event" | "fileop" | "bag"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Annotation {
    pub ts: String,            // created-at
    pub source: String,        // human|agent|rule|workflow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    pub target: Target,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<Value>,
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

pub fn list(bag: &Path) -> anyhow::Result<Vec<Value>> {
    let p = bag.join("annotations.ndjson");
    if !p.exists() { return Ok(vec![]); }
    let f = fs::File::open(&p)?;
    let br = std::io::BufReader::new(f);
    let mut out = Vec::new();
    for line in br.lines() {
        let s = match line { Ok(s) if !s.trim().is_empty() => s, _ => continue };
        if let Ok(v) = serde_json::from_str::<Value>(&s) { out.push(v); }
    }
    Ok(out)
}

pub fn add_for_event(
    bag: &Path,
    seq: u64,
    labels: Vec<String>,
    note: Option<String>,
    author: Option<String>,
    source: String,
    confidence: Option<f64>,
    attrs: Option<Value>,
) -> anyhow::Result<Annotation> {
    let row = pick_row(bag.join("events.ndjson").as_path(), seq)
        .with_context(|| "events seq not found")?;
    let ann = Annotation {
        ts: now_rfc3339(),
        source,
        author,
        labels,
        note,
        confidence,
        target: Target {
            kind: "event".into(),
            seq: Some(seq),
            ts: row.get("ts").and_then(|x| x.as_str()).map(|s| s.to_string()),
            run_id: row.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
            plugin: row.get("plugin").and_then(|x| x.as_str()).map(|s| s.to_string()),
            command: row.get("command").and_then(|x| x.as_str()).map(|s| s.to_string()),
            op: None, path: None,
        },
        attrs,
    };
    append(bag, &ann)?;
    Ok(ann)
}

pub fn add_for_fileop(
    bag: &Path,
    seq: u64,
    labels: Vec<String>,
    note: Option<String>,
    author: Option<String>,
    source: String,
    confidence: Option<f64>,
    attrs: Option<Value>,
) -> anyhow::Result<Annotation> {
    let row = pick_row(bag.join("fileops.ndjson").as_path(), seq)
        .with_context(|| "fileops seq not found")?;
    let ann = Annotation {
        ts: now_rfc3339(),
        source,
        author,
        labels,
        note,
        confidence,
        target: Target {
            kind: "fileop".into(),
            seq: Some(seq),
            ts: row.get("ts").and_then(|x| x.as_str()).map(|s| s.to_string()),
            run_id: row.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
            plugin: None, command: None,
            op: row.get("op").and_then(|x| x.as_str()).map(|s| s.to_string()),
            path: row.get("path").and_then(|x| x.as_str()).map(|s| s.to_string()),
        },
        attrs,
    };
    append(bag, &ann)?;
    Ok(ann)
}

pub fn add_for_bag(
    bag: &Path,
    labels: Vec<String>,
    note: Option<String>,
    author: Option<String>,
    source: String,
    confidence: Option<f64>,
    attrs: Option<Value>,
) -> anyhow::Result<Annotation> {
    // Try to read bag manifest for run_id
    let run_id = fs::read_to_string(bag.join("manifest.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()));

    let ann = Annotation {
        ts: now_rfc3339(),
        source,
        author,
        labels,
        note,
        confidence,
        target: Target {
            kind: "bag".into(),
            seq: None,
            ts: None,
            run_id,
            plugin: None, command: None, op: None, path: None,
        },
        attrs,
    };
    append(bag, &ann)?;
    Ok(ann)
}

fn pick_row(path: &Path, seq: u64) -> anyhow::Result<Value> {
    let f = fs::File::open(path)?;
    let br = std::io::BufReader::new(f);
    let mut idx = 0u64;
    for line in br.lines() {
        let s = line?;
        if s.trim().is_empty() { continue; }
        if idx == seq {
            let v: Value = serde_json::from_str(&s)?;
            return Ok(v);
        }
        idx += 1;
    }
    anyhow::bail!("seq {} out of bounds for {}", seq, path.display());
}

pub fn append(bag: &Path, ann: &Annotation) -> anyhow::Result<()> {
    let p = bag.join("annotations.ndjson");
    if let Some(parent) = p.parent() { fs::create_dir_all(parent)?; }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&p)?;
    writeln!(f, "{}", serde_json::to_string(ann)?)?;
    Ok(())
}


⸻

Patch 2 — Enhance exporter (join + annotations CSV)

Update plugins/replay/src/export.rs:

// add at top
use std::collections::{BTreeSet, HashMap};
use crate::annotate::{Annotation, Target};

// NEW: load annotations
fn load_annotations(bag: &Path) -> anyhow::Result<Vec<Annotation>> {
    let p = bag.join("annotations.ndjson");
    if !p.exists() { return Ok(vec![]); }
    let f = fs::File::open(&p)?;
    let br = std::io::BufReader::new(f);
    let mut out = Vec::new();
    for line in br.lines() {
        let s = match line { Ok(s) if !s.trim().is_empty() => s, _ => continue };
        if let Ok(a) = serde_json::from_str::<Annotation>(&s) { out.push(a); }
    }
    Ok(out)
}

fn aggregate_annotations(list: &[&Annotation]) -> (String,String,String,String,String,usize) {
    let mut labels = BTreeSet::<String>::new();
    let mut notes: Vec<String> = vec![];
    let mut authors = BTreeSet::<String>::new();
    let mut sources = BTreeSet::<String>::new();
    let mut sum = 0.0; let mut cnt = 0usize;

    for a in list {
        for l in &a.labels { labels.insert(l.clone()); }
        if let Some(n) = &a.note { if !n.is_empty() { notes.push(n.clone()); } }
        if let Some(x) = &a.author { if !x.is_empty() { authors.insert(x.clone()); } }
        if !a.source.is_empty() { sources.insert(a.source.clone()); }
        if let Some(c) = a.confidence { sum += c; cnt += 1; }
    }
    let conf = if cnt > 0 { format!("{:.3}", sum / (cnt as f64)) } else { "".to_string() };
    (
        labels.into_iter().collect::<Vec<_>>().join("|"),
        notes.join(" | "),
        authors.into_iter().collect::<Vec<_>>().join("|"),
        sources.into_iter().collect::<Vec<_>>().join("|"),
        conf,
        list.len()
    )
}

// NEW: annotations → CSV
pub fn export_annotations_csv(bag: &Path, out_path: &Path) -> anyhow::Result<usize> {
    let anns = load_annotations(bag)?;
    if let Some(parent) = out_path.parent() { fs::create_dir_all(parent)?; }
    let mut w = WriterBuilder::new().from_path(out_path)?;
    w.write_record([
        "ann_ts","source","author","labels","note","confidence",
        "target_kind","target_seq","target_ts","target_run_id","target_plugin","target_command","target_op","target_path","attrs_json"
    ])?;
    let mut n = 0usize;
    for a in &anns {
        let t = &a.target;
        w.write_record([
            a.ts.clone(),
            a.source.clone(),
            a.author.clone().unwrap_or_default(),
            a.labels.join("|"),
            a.note.clone().unwrap_or_default(),
            a.confidence.map(|x| format!("{:.3}", x)).unwrap_or_default(),
            t.kind.clone(),
            t.seq.map(|x| x.to_string()).unwrap_or_default(),
            t.ts.clone().unwrap_or_default(),
            t.run_id.clone().unwrap_or_default(),
            t.plugin.clone().unwrap_or_default(),
            t.command.clone().unwrap_or_default(),
            t.op.clone().unwrap_or_default(),
            t.path.clone().unwrap_or_default(),
            a.attrs.as_ref().map(|v| v.to_string()).unwrap_or_default(),
        ])?;
        n += 1;
    }
    w.flush()?;
    Ok(n)
}

// NEW: join annotations → events_annotated.csv
pub fn join_events_with_annotations(bag: &Path, out_dir: &Path) -> anyhow::Result<PathBuf> {
    let base = out_dir.join("events.csv");
    if !base.exists() {
        export_events_csv(bag, &base)?;
    }
    let anns = load_annotations(bag)?;
    if anns.is_empty() { return Ok(base); }

    let mut map: HashMap<u64, Vec<&Annotation>> = HashMap::new();
    for a in &anns {
        if a.target.kind == "event" {
            if let Some(seq) = a.target.seq {
                map.entry(seq).or_default().push(a);
            }
        }
    }

    let reader = fs::File::open(&base)?;
    let mut r = csv::Reader::from_reader(reader);
    let dst = out_dir.join("events_annotated.csv");
    let mut w = csv::Writer::from_path(&dst)?;

    let mut hdr: Vec<String> = r.headers()?.iter().map(|s| s.to_string()).collect();
    hdr.extend(["seq","ann_labels","ann_note","ann_author","ann_source","ann_confidence","ann_count"].map(|s| s.to_string()));
    w.write_record(&hdr)?;

    for (idx, rec) in r.records().enumerate() {
        let rec = rec?;
        let mut row = rec.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let seq = idx as u64;
        if let Some(list) = map.get(&seq) {
            let (labels, note, author, source, conf, count) = aggregate_annotations(list);
            row.extend([seq.to_string(), labels, note, author, source, conf, count.to_string()]);
        } else {
            row.extend([seq.to_string(), "".into(), "".into(), "".into(), "".into(), "".into(), "0".into()]);
        }
        w.write_record(&row)?;
    }
    w.flush()?;
    Ok(dst)
}

// NEW: join annotations → fileops_annotated.csv
pub fn join_fileops_with_annotations(bag: &Path, out_dir: &Path) -> anyhow::Result<PathBuf> {
    let base = out_dir.join("fileops.csv");
    if !base.exists() {
        export_fileops_csv(bag, &base)?;
    }
    let anns = load_annotations(bag)?;
    if anns.is_empty() { return Ok(base); }

    let mut map: HashMap<u64, Vec<&Annotation>> = HashMap::new();
    for a in &anns {
        if a.target.kind == "fileop" {
            if let Some(seq) = a.target.seq {
                map.entry(seq).or_default().push(a);
            }
        }
    }

    let reader = fs::File::open(&base)?;
    let mut r = csv::Reader::from_reader(reader);
    let dst = out_dir.join("fileops_annotated.csv");
    let mut w = csv::Writer::from_path(&dst)?;

    let mut hdr: Vec<String> = r.headers()?.iter().map(|s| s.to_string()).collect();
    hdr.extend(["seq","ann_labels","ann_note","ann_author","ann_source","ann_confidence","ann_count"].map(|s| s.to_string()));
    w.write_record(&hdr)?;

    for (idx, rec) in r.records().enumerate() {
        let rec = rec?;
        let mut row = rec.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let seq = idx as u64;
        if let Some(list) = map.get(&seq) {
            let (labels, note, author, source, conf, count) = aggregate_annotations(list);
            row.extend([seq.to_string(), labels, note, author, source, conf, count.to_string()]);
        } else {
            row.extend([seq.to_string(), "".into(), "".into(), "".into(), "".into(), "".into(), "0".into()]);
        }
        w.write_record(&row)?;
    }
    w.flush()?;
    Ok(dst)
}

Update function signature to support join and annotations:

// was: pub fn export_csv(bag: &Path, what: &str, out: Option<&Path>) -> ...
pub fn export_csv(bag: &Path, what: &str, out: Option<&Path>, join: Option<&str>)
    -> anyhow::Result<(Vec<PathBuf>, ExportCounts)>
{
    let out_dir = out.map(|p| p.to_path_buf())
        .unwrap_or_else(|| bag.join("exports").join("csv"));
    fs::create_dir_all(&out_dir)?;
    let mut written = Vec::<PathBuf>::new();
    let mut counts = ExportCounts { events_rows: 0, fileops_rows: 0, skipped: 0 };

    let want_events      = what == "all" || what.contains("events");
    let want_fileops     = what == "all" || what.contains("fileops");
    let want_annotations = what == "all" || what.contains("annotations");

    if want_events {
        let dst = out_dir.join("events.csv");
        match export_events_csv(bag, &dst) {
            Ok(n) => { written.push(dst); counts.events_rows = n; }
            Err(_) => { counts.skipped += 1; }
        }
    }
    if want_fileops {
        let dst = out_dir.join("fileops.csv");
        match export_fileops_csv(bag, &dst) {
            Ok(n) => { written.push(dst); counts.fileops_rows = n; }
            Err(_) => { counts.skipped += 1; }
        }
    }
    if want_annotations {
        let dst = out_dir.join("annotations.csv");
        match export_annotations_csv(bag, &dst) {
            Ok(_n) => { written.push(dst); }
            Err(_) => { counts.skipped += 1; }
        }
    }

    if matches!(join, Some("annotations")) {
        if want_events || what == "all" {
            let p = join_events_with_annotations(bag, &out_dir)?;
            written.push(p);
        }
        if want_fileops || what == "all" {
            let p = join_fileops_with_annotations(bag, &out_dir)?;
            written.push(p);
        }
    }

    Ok((written, counts))
}


⸻

Patch 3 — Wire CLI (replay)

Edit plugins/replay/src/lib.rs (snippets):
	1.	mods & uses

mod export;
mod annotate; // NEW

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};

	2.	augment_commands() — add annotate group and extend export:

.subcommand(
  ClapCommand::new("annotate").about("Annotate a bag (add/list)")
    .subcommand(
      ClapCommand::new("add")
        .about("Add an annotation")
        .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
        .arg(Arg::new("target").long("target").required(true)
             .value_parser(["events","fileops","bag"]))
        .arg(Arg::new("seq").long("seq").value_parser(clap::value_parser!(u64))
             .help("0-based row index in events/fileops (required for events/fileops targets)"))
        .arg(Arg::new("label").long("label").action(ArgAction::Append).value_name("TEXT"))
        .arg(Arg::new("note").long("note").value_name("TEXT"))
        .arg(Arg::new("author").long("author").value_name("TEXT"))
        .arg(Arg::new("source").long("source").default_value("human"))
        .arg(Arg::new("confidence").long("confidence").value_parser(clap::value_parser!(f64)))
        .arg(Arg::new("attrs").long("attrs").value_name("JSON")
             .help(r#"Optional JSON object with extra fields"#))
    )
    .subcommand(
      ClapCommand::new("list")
        .about("List annotations")
        .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
    )
)
.subcommand(
  ClapCommand::new("export").about("Export bag data for analysis")
    .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
    .arg(Arg::new("what").long("what")
         .value_parser(["events","fileops","annotations","all"]).default_value("all"))
    .arg(Arg::new("as").long("as").value_name("csv").default_value("csv"))
    .arg(Arg::new("join").long("join").value_name("annotations")
         .help("Join annotations into exported CSVs"))
    .arg(Arg::new("out").long("out").value_name("PATH")
         .help("Output dir (default: <bag>/exports/csv)"))
)

	3.	execute() — handle new subcommands:

Some(("annotate", am)) => {
    match am.subcommand() {
        Some(("add", m)) => {
            let bag = std::path::PathBuf::from(m.get_one::<String>("bag").unwrap());
            let target = m.get_one::<String>("target").unwrap().as_str();
            let labels: Vec<String> = m
                .get_many::<String>("label")
                .map(|v| v.map(|s| s.to_string()).collect())
                .unwrap_or_else(|| vec![]);
            let note = m.get_one::<String>("note").cloned();
            let author = m.get_one::<String>("author").cloned();
            let source = m.get_one::<String>("source").cloned().unwrap_or_else(|| "human".into());
            let confidence = m.get_one::<f64>("confidence").copied();
            let attrs = m.get_one::<String>("attrs").and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

            use crate::annotate as ann;
            let out = match target {
                "events" => {
                    let seq = *m.get_one::<u64>("seq").expect("--seq required for target=events");
                    let a = ann::add_for_event(&bag, seq, labels, note, author, source, confidence, attrs)?;
                    serde_json::to_value(a)?
                }
                "fileops" => {
                    let seq = *m.get_one::<u64>("seq").expect("--seq required for target=fileops");
                    let a = ann::add_for_fileop(&bag, seq, labels, note, author, source, confidence, attrs)?;
                    serde_json::to_value(a)?
                }
                "bag" => {
                    let a = ann::add_for_bag(&bag, labels, note, author, source, confidence, attrs)?;
                    serde_json::to_value(a)?
                }
                _ => anyhow::bail!("invalid target"),
            };
            return Ok(ctx.ok("replay","annotate.add",&out));
        }
        Some(("list", m)) => {
            let bag = std::path::PathBuf::from(m.get_one::<String>("bag").unwrap());
            let items = crate::annotate::list(&bag)?;
            return Ok(ctx.ok("replay","annotate.list",&serde_json::json!({
                "bag": bag.display().to_string(),
                "count": items.len(),
                "items": items
            })));
        }
        _ => return Ok(ctx.err("replay","annotate","replay.annotate.unknown","unknown annotate command",None,2)),
    }
}

Some(("export", em)) => {
    let bag = em.get_one::<String>("bag").unwrap();
    let what = em.get_one::<String>("what").map(|s| s.as_str()).unwrap_or("all");
    let fmt  = em.get_one::<String>("as").map(|s| s.as_str()).unwrap_or("csv");
    let join = em.get_one::<String>("join").map(|s| s.as_str());
    let outp = em.get_one::<String>("out").cloned();

    let bagp = std::path::PathBuf::from(bag);
    if !bagp.is_dir() {
        return Ok(ctx.err("replay","export","replay.export.nodir","bag path is not a directory",None,2));
    }
    if fmt != "csv" {
        return Ok(ctx.err("replay","export","replay.export.badfmt","only --as csv is supported in v0",None,2));
    }

    let out_dir = outp.map(std::path::PathBuf::from);
    let (written, counts) = crate::export::export_csv(&bagp, what, out_dir.as_deref(), join)?;
    let exit = ctx.ok("replay","export", &serde_json::json!({
        "bag": bag,
        "format": "csv",
        "written": written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "counts": { "events": counts.events_rows, "fileops": counts.fileops_rows, "skipped": counts.skipped }
    }));
    Ok(exit)
}

	4.	tool_specs() — expose tools (optional):

ToolSpec{
  name:"replay.annotate.add".into(),
  about:"Add an annotation to a bag".into(),
  args_schema: schemars::schema_for!(serde_json::Value),
  result_schema: schemars::schema_for!(serde_json::Value),
},
ToolSpec{
  name:"replay.annotate.list".into(),
  about:"List annotations for a bag".into(),
  args_schema: schemars::schema_for!(serde_json::Value),
  result_schema: schemars::schema_for!(serde_json::Value),
},


⸻

Patch 4 — E2E test

File: nexoscli-bin/tests/replay_annotate_export.rs (new)

mod helpers; use helpers::*;
use std::fs;
use serde_json::json;

#[test]
fn replay_annotate_and_export() {
    let t = tmp();

    // Create a run that changes files to populate events/fileops
    run_json(t.path(), &["dev","init","--yes"]);
    run_json(t.path(), &["term","run","--","bash","-lc","git init && git config user.email t@e && git config user.name T && echo base> a.txt && git add a.txt && git commit -m init"]);
    run_json(t.path(), &["term","run","--","bash","-lc","echo x>> a.txt && echo y> b.txt && git add -A && git commit -m change && (git mv b.txt c.txt || true)"]);

    // Extract last term.run bag
    let events = fs::read_to_string(t.path().join(".nexoscli/log/events.jsonl")).unwrap();
    let last_run = events.lines().rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v.get("plugin").and_then(|x| x.as_str())==Some("term") && v.get("command").and_then(|x| x.as_str())==Some("run"))
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .expect("term.run event present");

    let bag = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id",&last_run,"--out",bag.to_str().unwrap()]);
    assert!(bag.join("events.ndjson").exists());
    assert!(bag.join("fileops.ndjson").exists());

    // Add annotations
    let _a1 = run_json(t.path(), &[
        "replay","annotate","add",
        "--bag", bag.to_str().unwrap(),
        "--target","events",
        "--seq","0",
        "--label","slow","--label","investigate",
        "--note","p90 high",
        "--author","tester",
        "--confidence","0.8"
    ]);
    let _a2 = run_json(t.path(), &[
        "replay","annotate","add",
        "--bag", bag.to_str().unwrap(),
        "--target","fileops",
        "--seq","0",
        "--label","write-hot"
    ]);

    // List annotations
    let vlist = run_json(t.path(), &["replay","annotate","list","--bag",bag.to_str().unwrap()]);
    assert_eq!(vlist["data"]["count"].as_u64().unwrap(), 2);

    // Export annotations CSV only
    run_json(t.path(), &["replay","export","--bag",bag.to_str().unwrap(),"--what","annotations"]);
    let ann_csv = bag.join("exports").join("csv").join("annotations.csv");
    assert!(ann_csv.exists());
    let ann_body = fs::read_to_string(&ann_csv).unwrap();
    assert!(ann_body.lines().count() >= 2);
    assert!(ann_body.contains("target_kind,target_seq"));

    // Export events joined with annotations
    run_json(t.path(), &["replay","export","--bag",bag.to_str().unwrap(),"--what","events","--join","annotations"]);
    let e_ann = bag.join("exports").join("csv").join("events_annotated.csv");
    assert!(e_ann.exists());
    let e_body = fs::read_to_string(&e_ann).unwrap();
    assert!(e_body.lines().next().unwrap().contains("ann_labels"));
    assert!(e_body.contains("investigate"));

    // Export fileops joined with annotations
    run_json(t.path(), &["replay","export","--bag",bag.to_str().unwrap(),"--what","fileops","--join","annotations"]);
    let f_ann = bag.join("exports").join("csv").join("fileops_annotated.csv");
    assert!(f_ann.exists());
    let f_body = fs::read_to_string(&f_ann).unwrap();
    assert!(f_body.lines().next().unwrap().contains("ann_labels"));
}


⸻

Patch 5 — Docs (TRACEPACK.md)

Append a new section:

### Annotations (v0)

You can attach labels/notes to a bag, an event row, or a fileop row.

**Add & list**
```bash
# add to an event row (by sequence index)
nexoscli replay annotate add --bag ./bag --target events  --seq 0 \
  --label slow --label investigate --note "p90 high" --author you --confidence 0.8

# add to a fileop row
nexoscli replay annotate add --bag ./bag --target fileops --seq 0 --label write-hot

# bag-level note
nexoscli replay annotate add --bag ./bag --target bag --label reviewed --note "LGTM"

# list
nexoscli replay annotate list --bag ./bag --output json

Export

# annotations as CSV
nexoscli replay export --bag ./bag --what annotations

# join annotations into CSVs (adds columns: seq, ann_* )
nexoscli replay export --bag ./bag --what events   --join annotations
nexoscli replay export --bag ./bag --what fileops  --join annotations

annotations.ndjson schema keeps target.seq as the join key, plus helpful context copies (ts/run_id/plugin/…).

---

## Smoke run

```bash
cargo build --workspace

# Generate a bag
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli term run -- bash -lc 'git init && git config user.email t@e && git config user.name T && echo hello> a && git add a && git commit -m init'
RUN=$(tail -n 1 .nexoscli/log/events.jsonl | jq -r '.run_id')
./target/debug/nexoscli replay extract --run-id "$RUN" --out bag

# Annotate
./target/debug/nexoscli replay annotate add --bag bag --target events  --seq 0 --label slow --note "check p90"
./target/debug/nexoscli replay annotate add --bag bag --target fileops --seq 0 --label write-hot

# Export
./target/debug/nexoscli replay export --bag bag --what all --join annotations
ls bag/exports/csv
# annotations.csv events.csv events_annotated.csv fileops.csv fileops_annotated.csv


⸻

Why this is the right v0
	•	Simple & durable: one annotations.ndjson per bag; append-only; line‑delimited JSON that tools already grok.
	•	Analyst-ready: CSV outputs for events/fileops/annotations; joined CSVs add ann_* columns without changing the base CSV schema (#36 stays stable).
	•	Composable: can be extended with more join keys (e.g., SHA digests) or Parquet exports later without breaking v0.

⸻