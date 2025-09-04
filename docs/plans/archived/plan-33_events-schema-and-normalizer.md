---
Implemented in: commit afcecc8d6d6aa479d9dab4da5206672c776f5061 feat(replay): normalize to events v0.3 + schema surfaces (TASK-20250904-plan-33-events-schema-v0-3-normalizers)
---

Awesome — here’s the next increment:

⸻

Implementation Plan #33 — “Events Schema v0.3 + Normalizers”

Why: lock down a stable, analysis‑ready event record without touching protected core files. We add a canonical schema (v0.3) and a normalizer that transforms any current logs/bags into that schema. This gives downstream analysis a single dependable shape and lets CI verify/compare runs across time.

✅ We do not edit protected files (nexoscli-core/src/event.rs, etc.).
✅ Everything lives in the replay plugin, with tests and docs.
✅ Compatible with existing bags and logs.

⸻

1) What “Events v0.3” looks like

Each line is one event in NDJSON:

{
  "schema": "nexos/events@0.3",
  "ts": "2025-09-02T07:29:30Z",     // RFC3339
  "run_id": "RUN-ABC123",
  "plugin": "term",
  "command": "run",
  "source": "term.run",             // derived convenience field
  "ok": true,                       // exit_code == 0
  "exit_code": 0,
  "dur_ms": 1250,                   // optional
  "args_hash": "sha256:...",
  "content_hash": "sha256:...",
  "context": {                      // if available in original event
    "task_id": "TASK-...",
    "stage": "implementation",
    "session_id": "SES-..."
  },
  "attrs": {                        // event-specific extras (pass-through)
    "cmd": "make test",
    "cwd": "/path/to/project"
  }
}

Notes
	•	schema is fixed string: "nexos/events@0.3"
	•	Missing optional fields are simply omitted.
	•	All non-standard fields from older logs are preserved inside attrs.

⸻

2) New CLI surfaces (additive)

A) Normalize events to v0.3

nexoscli replay normalize --to 0.3 \
  [--bag PATH | --events PATH] \
  [--run-id RUN-...] \
  [--out PATH] \
  [--update-manifest]

	•	Input: either a TracePack directory/.tgz via --bag, or a raw .nexoscli/log/events.jsonl via --events.
	•	Output: events.v0.3.ndjson (default next to source) or --out PATH.
	•	--run-id filters when normalizing a shared log file.
	•	--update-manifest (only with --bag) adds:

"events_v03": {
  "schema": "nexos/events@0.3",
  "dst": "events.v0.3.ndjson",
  "lines": 123,
  "bytes": 4567,
  "sha256": "sha256:..."
}

Envelope (JSON):

{
  "data": {
    "schema": "nexos/events@0.3",
    "in_lines": 321,
    "out_lines": 321,
    "out": "tracepack/events.v0.3.ndjson",
    "run_id": "RUN-ABC123",
    "sha256": "sha256:..."
  }
}

B) Print the v0.3 schema (for agents/tools)

nexoscli replay schema events --version 0.3 --output json

Returns the JSON Schema generated via schemars and an inlined example.

⸻

3) Code changes (replay plugin)

Scope: plugins/replay/**, nexoscli-bin/tests/**, docs/**.
No protected core changes.

plugins/replay/Cargo.toml (add deps)

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow        = { workspace = true }
serde         = { workspace = true, features = ["derive"] }
serde_json    = { workspace = true }
schemars      = { workspace = true }
clap          = { workspace = true }
time          = { workspace = true }
walkdir       = { workspace = true }
sha2          = { workspace = true }
hex           = { workspace = true }
flate2        = "1"      # read .tgz bags
tar           = "0.4"

plugins/replay/src/lib.rs (new subcommands + normalizer)

Add under the existing replay plugin:

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{plugin::{Plugin, CommandContext, ToolSpec}, Result};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, io::Read, path::{Path, PathBuf}};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use sha2::{Digest, Sha256};
use flate2::read::GzDecoder;
use tar::Archive;

/* ---------------- v0.3 model ---------------- */

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct V03Context {
    #[serde(skip_serializing_if="Option::is_none")] task_id: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] stage: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct V03Event {
    schema: String, // "nexos/events@0.3"
    ts: String,
    run_id: String,
    plugin: String,
    command: String,
    source: String,
    ok: bool,
    exit_code: i32,
    #[serde(skip_serializing_if="Option::is_none")] dur_ms: Option<u64>,
    #[serde(skip_serializing_if="Option::is_none")] args_hash: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] content_hash: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] context: Option<V03Context>,
    #[serde(skip_serializing_if="Option::is_none")] attrs: Option<Value>,
}

/* ---------------- CLI args/results ---------------- */

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct NormalizeArgs {
    #[serde(default)] bag: Option<String>,
    #[serde(default)] events: Option<String>,
    #[serde(default)] run_id: Option<String>,
    #[serde(default)] out: Option<String>,
    to: String, // "0.3"
    #[serde(default)] update_manifest: bool,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct NormalizeResult {
    schema: String,
    in_lines: u64,
    out_lines: u64,
    out: String,
    #[serde(skip_serializing_if="Option::is_none")] run_id: Option<String>,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaArgs {
    what: String, // "events"
    version: String, // "0.3"
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaResult {
    name: String,
    version: String,
    schema: Value,
    example: Value,
}

/* ---------------- Plugin surface ---------------- */

pub struct ReplayPlugin;
impl Plugin for ReplayPlugin {
    fn name(&self) -> &'static str { "replay" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Replay & TracePack utilities" }

    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("replay").about(self.describe())
              // ... (existing subcommands: extract/plan/run/verify etc.)
              .subcommand(
                  ClapCommand::new("normalize")
                    .about("Normalize events to a stable schema (v0.3)")
                    .arg(Arg::new("to").long("to").required(true).value_parser(["0.3"]))
                    .arg(Arg::new("bag").long("bag").value_name("PATH"))
                    .arg(Arg::new("events").long("events").value_name("PATH"))
                    .arg(Arg::new("run-id").long("run-id").value_name("RUN"))
                    .arg(Arg::new("out").long("out").value_name("PATH"))
                    .arg(Arg::new("update-manifest").long("update-manifest").action(ArgAction::SetTrue))
              )
              .subcommand(
                  ClapCommand::new("schema")
                    .about("Print schema for replay artifacts")
                    .subcommand(
                        ClapCommand::new("events")
                          .about("JSON Schema for events v0.3")
                          .arg(Arg::new("version").long("version").required(true).value_parser(["0.3"]))
                    )
              )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            // --- replay normalize ---
            Some(("normalize", sm)) => {
                let to = sm.get_one::<String>("to").unwrap().to_string();
                let bag = sm.get_one::<String>("bag").cloned();
                let events = sm.get_one::<String>("events").cloned();
                let run_id = sm.get_one::<String>("run-id").cloned();
                let out = sm.get_one::<String>("out").cloned();
                let update_manifest = sm.get_flag("update-manifest");

                // sanity
                if bag.is_none() && events.is_none() {
                    return Ok(ctx.err("replay","normalize","replay.normalize.args","pass either --bag or --events", None, 2));
                }
                if bag.is_some() && events.is_some() {
                    return Ok(ctx.err("replay","normalize","replay.normalize.args","pass only one of --bag or --events", None, 2));
                }
                if to != "0.3" {
                    return Ok(ctx.err("replay","normalize","replay.normalize.unsupported","only v0.3 supported", None, 2));
                }

                let res = if let Some(b) = bag { normalize_from_bag(&b, out, update_manifest)? }
                          else { normalize_from_events(&events.unwrap(), run_id, out)? };

                Ok(ctx.ok("replay","normalize",&res))
            }

            // --- replay schema events ---
            Some(("schema", sm)) => {
                if let Some(("events", ev)) = sm.subcommand() {
                    let _v = ev.get_one::<String>("version").unwrap(); // only 0.3
                    let schema = schema_for!(V03Event);
                    let example = json!({
                        "schema":"nexos/events@0.3","ts":"2025-09-02T07:29:30Z","run_id":"RUN-EX",
                        "plugin":"term","command":"run","source":"term.run","ok":true,"exit_code":0,
                        "dur_ms":123,"args_hash":"sha256:...","content_hash":"sha256:...",
                        "context":{"task_id":"TASK-...","stage":"implementation","session_id":"SES-..."},
                        "attrs":{"cmd":"make test","cwd":"..."}
                    });
                    let out = SchemaResult{
                        name:"events".into(), version:"0.3".into(),
                        schema: serde_json::to_value(schema).unwrap(),
                        example
                    };
                    return Ok(ctx.ok("replay","schema.events",&out));
                }
                Ok(ctx.err("replay","schema","replay.schema.unknown","unknown schema",None,2))
            }

            _ => Ok(2),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            // … keep existing ToolSpec entries
            ToolSpec {
                name:"replay.normalize".into(),
                about:"Normalize events → v0.3 (from bag or events file)".into(),
                args_schema: schema_for!(NormalizeArgs),
                result_schema: schema_for!(NormalizeResult),
            },
            ToolSpec {
                name:"replay.schema.events".into(),
                about:"JSON Schema for events v0.3".into(),
                args_schema: schema_for!(SchemaArgs),
                result_schema: schema_for!(SchemaResult),
            },
        ]
    }
}

/* ---------------- impl: normalize ---------------- */

enum Bag { Dir(PathBuf), Tgz(Vec<(String, Vec<u8>)>) }

fn load_bag(path: &Path) -> anyhow::Result<Bag> {
    if path.is_dir() { return Ok(Bag::Dir(path.to_path_buf())); }
    let s = path.to_string_lossy();
    if s.ends_with(".tgz") || s.ends_with(".tar.gz") {
        let f = fs::File::open(path)?;
        let mut ar = Archive::new(GzDecoder::new(f));
        let mut files = Vec::<(String, Vec<u8>)>::new();
        for e in ar.entries()? {
            let mut e = e?;
            let mut buf = Vec::new();
            e.read_to_end(&mut buf)?;
            let p = e.path()?.to_string_lossy().to_string().replace('\\',"/");
            files.push((p, buf));
        }
        return Ok(Bag::Tgz(files));
    }
    anyhow::bail!("not a directory or .tgz: {}", path.display())
}
fn read_to_string(bag: &Bag, rel: &str) -> Option<String> {
    match bag {
        Bag::Dir(d) => fs::read_to_string(d.join(rel)).ok(),
        Bag::Tgz(v) => v.iter().find(|(p,_)| p == rel).map(|(_,b)| String::from_utf8_lossy(b).to_string()),
    }
}
fn write_string(path: &Path, s: &str) -> anyhow::Result<()> {
    if let Some(par) = path.parent() { fs::create_dir_all(par)?; }
    fs::write(path, s.as_bytes())?;
    Ok(())
}
fn sha256_bytes(b: &[u8]) -> String {
    let mut h = Sha256::new(); h.update(b); format!("sha256:{:x}", h.finalize())
}

fn to_v03(v: &Value) -> Option<V03Event> {
    // Required base
    let ts = v.get("ts").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let run_id = v.get("run_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let plugin = v.get("plugin").and_then(|x| x.as_str()).unwrap_or("-").to_string();
    let command = v.get("command").and_then(|x| x.as_str()).unwrap_or("-").to_string();
    if ts.is_empty() || run_id.is_empty() { return None; }

    let exit_code_i = v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
    let ok = exit_code_i == 0;
    let dur_ms = v.get("dur_ms").and_then(|x| x.as_u64());
    let args_hash = v.get("args_hash").and_then(|x| x.as_str()).map(|s| s.to_string());
    let content_hash = v.get("content_hash").and_then(|x| x.as_str()).map(|s| s.to_string());

    // Context: prefer top-level "context", else meta.context
    let context = v.get("context")
        .or_else(|| v.get("meta").and_then(|m| m.get("context")))
        .and_then(|c| {
            Some(V03Context{
                task_id: c.get("task_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
                stage: c.get("stage").and_then(|x| x.as_str()).map(|s| s.to_string()),
                session_id: c.get("session_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
            })
        });

    // attrs: shallow pass-through
    let attrs = v.get("attrs").cloned();

    Some(V03Event{
        schema: "nexos/events@0.3".into(),
        ts, run_id,
        plugin: plugin.clone(),
        command: command.clone(),
        source: format!("{}.{}", plugin, command),
        ok, exit_code: exit_code_i,
        dur_ms, args_hash, content_hash,
        context, attrs
    })
}

fn normalize_from_bag(bag_path: &str, out: Option<String>, update_manifest: bool) -> anyhow::Result<NormalizeResult> {
    let bag = load_bag(Path::new(bag_path))?;
    // read manifest to confirm events path (assume "events.ndjson")
    let man_txt = read_to_string(&bag, "manifest.json").ok_or_else(|| anyhow::anyhow!("manifest.json missing in bag"))?;
    let man: Value = serde_json::from_str(&man_txt)?;
    let events_rel = "events.ndjson";
    let ev_txt = read_to_string(&bag, events_rel).ok_or_else(|| anyhow::anyhow!("{} missing", events_rel))?;

    let mut out_lines = 0u64;
    let mut in_lines = 0u64;
    let mut run_id_line: Option<String> = None;
    let mut buf = String::new();

    for line in ev_txt.lines() {
        if line.trim().is_empty() { continue; }
        in_lines += 1;
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            if run_id_line.is_none() { run_id_line = v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()); }
            if let Some(n) = to_v03(&v) {
                buf.push_str(&serde_json::to_string(&n)?); buf.push('\n');
                out_lines += 1;
            }
        }
    }

    // write out
    let out_path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(bag_path).join("events.v0.3.ndjson"));
    write_string(&out_path, &buf)?;
    let sha = sha256_bytes(buf.as_bytes());

    // optionally update manifest
    if update_manifest {
        let mut man_obj = man.as_object().cloned().unwrap_or_default();
        man_obj.insert("events_v03".into(), json!({
            "schema":"nexos/events@0.3",
            "dst":"events.v0.3.ndjson",
            "lines": out_lines,
            "bytes": buf.as_bytes().len() as u64,
            "sha256": sha
        }));
        write_string(&Path::new(bag_path).join("manifest.json"), &serde_json::to_string_pretty(&man_obj)?)?;
    }

    Ok(NormalizeResult{
        schema:"nexos/events@0.3".into(),
        in_lines, out_lines,
        out: out_path.display().to_string(),
        run_id: run_id_line,
        sha256: sha
    })
}

fn normalize_from_events(events_path: &str, run_id: Option<String>, out: Option<String>) -> anyhow::Result<NormalizeResult> {
    let txt = fs::read_to_string(events_path)?;
    let mut out_lines = 0u64;
    let mut in_lines = 0u64;
    let mut buf = String::new();
    let mut chosen_run = run_id.clone();

    for line in txt.lines() {
        if line.trim().is_empty() { continue; }
        in_lines += 1;
        let v: Value = match serde_json::from_str(line) { Ok(v)=>v, Err(_)=>continue };
        let rid = v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string());
        if chosen_run.is_none() { chosen_run = rid.clone(); }
        if let Some(filter) = run_id.as_ref() {
            if rid.as_deref() != Some(filter.as_str()) { continue; }
        }
        if let Some(n) = to_v03(&v) {
            buf.push_str(&serde_json::to_string(&n)?); buf.push('\n');
            out_lines += 1;
        }
    }

    let out_path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(events_path).with_file_name("events.v0.3.ndjson"));
    write_string(&out_path, &buf)?;
    let sha = sha256_bytes(buf.as_bytes());

    Ok(NormalizeResult{
        schema:"nexos/events@0.3".into(),
        in_lines, out_lines,
        out: out_path.display().to_string(),
        run_id: chosen_run,
        sha256: sha
    })
}

The normalizer is lossless for analysis: all non-standard keys stay under attrs. It gracefully handles older lines missing optional fields.

⸻

4) Tests

nexoscli-bin/tests/replay_normalize_v03.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn normalize_from_bag_writes_v03() {
    let t = tmp();

    // produce a run & bag
    let v = run_json(t.path(), &["term","run","--","sh","-c","echo hi"]);
    let run_id = v["meta"]["run_id"].as_str().unwrap();
    let outdir = t.path().join("bag");
    let _ = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", outdir.to_str().unwrap()]);

    // normalize inside bag
    let rep = run_json(t.path(), &["replay","normalize","--to","0.3","--bag", outdir.to_str().unwrap(), "--update-manifest"]);
    assert_eq!(rep["data"]["schema"], "nexos/events@0.3");
    let outp = rep["data"]["out"].as_str().unwrap();
    assert!(t.path().join(outp).exists() || std::path::Path::new(outp).exists());

    // verify file looks like v0.3
    let body = fs::read_to_string(outp).unwrap();
    let line = body.lines().next().unwrap();
    let v = serde_json::from_str::<serde_json::Value>(line).unwrap();
    assert_eq!(v["schema"].as_str().unwrap(), "nexos/events@0.3");
    assert!(v["source"].as_str().unwrap().contains("."));
}

#[test]
fn normalize_from_events_filters_run() {
    let t = tmp();
    // generate two runs
    let r1 = run_json(t.path(), &["term","run","--","sh","-c","echo one"]);
    let run1 = r1["meta"]["run_id"].as_str().unwrap().to_string();
    let r2 = run_json(t.path(), &["term","run","--","sh","-c","echo two"]);
    let run2 = r2["meta"]["run_id"].as_str().unwrap().to_string();

    // normalize only run1
    let log = t.path().join(".nexoscli/log/events.jsonl");
    let rep = run_json(t.path(), &["replay","normalize","--to","0.3","--events", log.to_str().unwrap(), "--run-id", &run1]);
    let outp = rep["data"]["out"].as_str().unwrap();
    let s = fs::read_to_string(outp).unwrap();
    assert!(s.lines().all(|l| {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        v["run_id"].as_str().unwrap() == run1
    }));
    assert!(s.lines().count() >= 1);
    assert!(s.lines().count() < fs::read_to_string(log).unwrap().lines().count());
}


⸻

5) Docs

docs/EVENTS_SCHEMA.md (new)
	•	Explain v0.3 (fields, invariants, examples).
	•	Explain normalization: from v0.1/v0.2 → v0.3 mapping.
	•	Show how to consume in Python (tiny snippet from your earlier “Bags for Analysts” doc, but focused on events).

docs/TRACEPACK.md (append)

Add a section “Normalize events to v0.3”:

```bash
# Inside a bag dir or .tgz
nexoscli replay normalize --to 0.3 --bag tracepack/ --update-manifest
# or from raw log (filter by run):
nexoscli replay normalize --to 0.3 --events .nexoscli/log/events.jsonl --run-id RUN-...

---

## 6) Smoke

```bash
cargo build --workspace

# Generate a run
./target/debug/nexoscli term run -- echo hi
RUN=$(jq -r '.run_id' .nexoscli/log/events.jsonl | tail -1)

# Extract a bag
./target/debug/nexoscli replay extract --run-id "$RUN" --out bag

# Normalize inside bag + manifest update
./target/debug/nexoscli replay normalize --to 0.3 --bag bag --update-manifest --output json | jq

# Print schema
./target/debug/nexoscli replay schema events --version 0.3 --output json | jq '.data.example'
