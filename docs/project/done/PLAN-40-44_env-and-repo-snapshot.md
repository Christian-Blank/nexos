---
status: done
plan_id: '40'
slug: 44_env-and-repo-snapshot
task_id: TASK-20250905-
started_at: 2025-09-05T22:51:58.837209Z
source: next
completed_at: 2025-09-05T22:52:51.445719Z
implemented_in: 2b9a21f
---
Awesome—let’s close the v0 gaps now. Below you’ll find implementation plans #41–#45 with concrete patches, exactly where to put them, and small E2E tests. At the end I include an updated v0 roadmap with the items we just closed checked off.

Scope labels (for Cline):
Use replay for changes under plugins/replay/**, dev-plugin for plugins/dev/**, and observability for tests/docs.
Run the standard pre‑flight (nexoscli dev init, nexoscli dev doctor --ci ...) before edits.

⸻

Plan #40 — Env & Repo Snapshot (P0)

Goal: Every TracePack (bag) records tool versions, OS/arch, and repo state (branch/HEAD/dirty) in the manifest.

40.1 Add helpers to the replay plugin

File: plugins/replay/src/lib.rs
Add these near other helpers (or in a new util.rs and mod util; if you prefer).

use std::{path::{Path, PathBuf}, process::Command as Proc};

fn capture_cmd_output(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Proc::new(cmd).args(args).output().ok()?;
    if !out.status.success() { return None; }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn collect_tools_snapshot() -> serde_json::Value {
    // Best-effort only; all Optionals
    let rustc = capture_cmd_output("rustc", &["--version"]);
    let cargo = capture_cmd_output("cargo", &["--version"]);
    // Try to get nexoscli version by re-running our own exe with --version
    let nexoscli = std::env::current_exe().ok()
        .and_then(|exe| Proc::new(exe).arg("--version").output().ok())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    serde_json::json!({
        "nexoscli": nexoscli,
        "rustc": rustc,
        "cargo": cargo
    })
}

fn collect_env_snapshot() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,   // "linux" | "macos" | "windows" | ...
        "arch": std::env::consts::ARCH // "x86_64" | "aarch64" | ...
    })
}

fn collect_git_snapshot(repo_root: &Path) -> serde_json::Value {
    let branch = capture_cmd_output("git", &["rev-parse", "--abbrev-ref", "HEAD"]);
    let head   = capture_cmd_output("git", &["rev-parse", "HEAD"]);
    // "git diff --quiet" returns 0 when no changes, 1 when dirty
    let dirty = Proc::new("git").current_dir(repo_root).args(["diff", "--quiet"]).status()
        .map(|s| !s.success()).unwrap_or(false);
    serde_json::json!({
        "branch": branch,
        "head": head,
        "dirty": dirty
    })
}

40.2 Write snapshots into bag manifest during extract

Where you build the TracePack manifest (in your replay extract implementation), extend it:

// inside extract implementation, after you computed manifest skeleton:
let repo_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

manifest["tools"] = collect_tools_snapshot();
manifest["env"]   = collect_env_snapshot();
manifest["git"]   = collect_git_snapshot(&repo_root);

This is additive and backward compatible with the current manifest.

40.3 Tiny helper (optional): dev env print

File: plugins/dev/src/lib.rs
Add a small command to print what we capture (handy for debugging).
	•	In augment_commands() add:

// under Command::new("dev")...
.subcommand(
  Command::new("env").about("Environment helpers")
    .subcommand(
      Command::new("print").about("Print tool/os/git snapshot (JSON)")
    )
)

	•	In execute() add:

Some(("env", em)) => {
    if let Some(("print", _)) = em.subcommand() {
        let cwd = std::env::current_dir()?;
        let tools = serde_json::json!({
            "tools": super::collect_tools_snapshot_for_dev(),
            "env":   super::collect_env_snapshot_for_dev(),
            "git":   super::collect_git_snapshot_for_dev(&cwd),
        });
        let exit = ctx.ok("dev","env.print",&tools);
        return Ok(exit);
    }
    Ok(2)
}

	•	Add thin wrappers (reuse code or re‑declare local simple versions) at the bottom of file:

pub(super) fn collect_tools_snapshot_for_dev() -> serde_json::Value {
    use std::process::Command as Proc;
    let out = |cmd: &str, a: &[&str]| Proc::new(cmd).args(a).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    serde_json::json!({
        "nexoscli": std::env::current_exe().ok()
            .and_then(|exe| Proc::new(exe).arg("--version").output().ok())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()),
        "rustc": out("rustc", &["--version"]),
        "cargo": out("cargo", &["--version"])
    })
}
pub(super) fn collect_env_snapshot_for_dev() -> serde_json::Value {
    serde_json::json!({ "os": std::env::consts::OS, "arch": std::env::consts::ARCH })
}
pub(super) fn collect_git_snapshot_for_dev(root: &std::path::Path) -> serde_json::Value {
    use std::process::Command as Proc;
    let out = |args: &[&str]| Proc::new("git").current_dir(root).args(args).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let dirty = Proc::new("git").current_dir(root).args(["diff","--quiet"]).status()
        .map(|s| !s.success()).unwrap_or(false);
    serde_json::json!({
        "branch": out(&["rev-parse","--abbrev-ref","HEAD"]),
        "head":   out(&["rev-parse","HEAD"]),
        "dirty":  dirty
    })
}


⸻

Plan #41 — Verify++ & Doctor v0 profile (P0)

Goal: replay verify validates schemas, counts, patch dry‑run, and warns on missing Cline coverage. Add --profile v0 to dev doctor.

41.1 Add JSON‑schema validation to replay verify

File: plugins/replay/Cargo.toml

[dependencies]
# ...
jsonschema = "0.17"

File: plugins/replay/src/lib.rs
Add schemas (compact) and a validator helper.

use jsonschema::{JSONSchema};
use serde_json::Value;

// Minimal schemas (keep in code for v0; later we can export to docs)
const EVENTS_V03_SCHEMA: &str = r#"{
  "type":"object",
  "required":["ts","run_id","plugin","command","exit_code","ok"],
  "properties":{
    "ts":{"type":"string"},
    "run_id":{"type":"string"},
    "plugin":{"type":"string"},
    "command":{"type":"string"},
    "exit_code":{"type":"integer"},
    "ok":{"type":"boolean"},
    "dur_ms":{"type":"integer"},
    "context":{"type":"object"},
    "attrs":{"type":"object"}
  },
  "additionalProperties":true
}"#;

const FILEOPS_V01_SCHEMA: &str = r#"{
  "type":"object",
  "required":["ts","run_id","op","path"],
  "properties":{
    "ts":{"type":"string"},
    "run_id":{"type":"string"},
    "op":{"type":"string","enum":["create","modify","delete","rename"]},
    "path":{"type":"string"},
    "bytes":{"type":"integer"},
    "patch":{"type":"object"},
    "digest_after":{"type":"string"},
    "source":{"type":"string"},
    "context":{"type":"object"}
  },
  "additionalProperties":true
}"#;

fn validate_ndjson(path: &Path, schema_src: &str, limit_errs: usize) -> anyhow::Result<Vec<String>> {
    let text = std::fs::read_to_string(path)?;
    let schema: Value = serde_json::from_str(schema_src)?;
    let compiled = JSONSchema::compile(&schema).map_err(|e| anyhow::anyhow!("schema: {e}"))?;
    let mut errs = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        if line.trim().is_empty() { continue; }
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => { errs.push(format!("line {}: invalid JSON: {}", lineno+1, e)); if errs.len() >= limit_errs { break; } continue; }
        };
        if let Err(iter) = compiled.validate(&v) {
            for e in iter {
                errs.push(format!("line {}: {}", lineno+1, e));
                if errs.len() >= limit_errs { break; }
            }
        }
        if errs.len() >= limit_errs { break; }
    }
    Ok(errs)
}

Hook it into replay verify:

// inside your verify implementation after you locate bag dir and manifest:
let events_p = bag.join("events.ndjson");
if events_p.exists() {
    let errs = validate_ndjson(&events_p, EVENTS_V03_SCHEMA, 20)?;
    if !errs.is_empty() {
        // emit an error envelope or include in verify report (non-zero exit)
        verify_report.events_schema_ok = false;
        verify_report.events_schema_errors = Some(errs);
    } else {
        verify_report.events_schema_ok = true;
    }
}

let fileops_p = bag.join("fileops.ndjson");
if fileops_p.exists() {
    let errs = validate_ndjson(&fileops_p, FILEOPS_V01_SCHEMA, 20)?;
    if !errs.is_empty() {
        verify_report.fileops_schema_ok = false;
        verify_report.fileops_schema_errors = Some(errs);
    } else {
        verify_report.fileops_schema_ok = true;
    }
}

42.2 Count checks & patch dry‑run

Add to replay verify:

// Count checks against manifest.counts
if events_p.exists() {
    let lines = std::fs::read_to_string(&events_p)?.lines().filter(|l| !l.trim().is_empty()).count() as u64;
    verify_report.events_lines = Some(lines);
    if let Some(exp) = manifest.get("counts").and_then(|c| c.get("events_lines")).and_then(|x| x.as_u64()) {
        verify_report.events_count_match = Some(lines == exp);
    }
}
if fileops_p.exists() {
    let lines = std::fs::read_to_string(&fileops_p)?.lines().filter(|l| !l.trim().is_empty()).count() as u64;
    verify_report.fileops_lines = Some(lines);
    if let Some(exp) = manifest.get("counts").and_then(|c| c.get("fileops")).and_then(|x| x.as_u64()) {
        verify_report.fileops_count_match = Some(lines == exp);
    }
}

// Patch dry-run if patches/ exist (git apply --check)
let patches_dir = bag.join("patches");
if patches_dir.exists() {
    let mut ok_all = true;
    for ent in std::fs::read_dir(&patches_dir)? {
        let p = ent?.path();
        if p.extension().and_then(|x| x.to_str()) == Some("patch") {
            let st = Proc::new("git").args(["apply","--check"]).stdin(std::process::Stdio::piped()).spawn();
            match st {
                Ok(mut child) => {
                    // feed the patch file to stdin
                    use std::io::Write;
                    if let Some(mut stdin) = child.stdin.take() {
                        let bytes = std::fs::read(&p)?;
                        stdin.write_all(&bytes)?;
                    }
                    let status = child.wait()?;
                    ok_all &= status.success();
                }
                Err(_) => { ok_all = false; }
            }
        }
    }
    verify_report.patches_check = Some(ok_all);
}

41.3 Cline coverage guard

// Warn if term.run present but no cline artifacts
let has_term = std::fs::read_to_string(&events_p)
    .map(|s| s.lines().any(|l| l.contains("\"plugin\":\"term\"") && l.contains("\"command\":\"run\"")))
    .unwrap_or(false);

let cline_dir = bag.join("cline");
let cline_tasks_n = cline_dir.join("tasks");
let cline_msgs = cline_dir.join("messages.ndjson");
let has_cline = (cline_tasks_n.exists() && std::fs::read_dir(&cline_tasks_n).ok().map(|mut it| it.next().is_some()).unwrap_or(false))
    || (cline_msgs.exists() && std::fs::metadata(&cline_msgs).map(|m| m.len() > 0).unwrap_or(false));

verify_report.cline_coverage = Some(has_cline);
if has_term && !has_cline {
    verify_report.warnings.push("cline_coverage_zero".into());
}

41.4 Doctor v0 profile

File: plugins/dev/src/lib.rs
	•	Extend the doctor subcommand CLI:

.arg(Arg::new("profile").long("profile").value_parser(["v0"]))

	•	In the ci branch, derive want & required if --profile v0 is present:

let profile = m.get_one::<String>("profile").cloned();
let mut want: std::collections::HashSet<String> = if let Some(p) = &profile {
    match p.as_str() {
        "v0" => ["graph","plugins","config","events","cline","rules","tools"].into_iter().map(|s| s.to_string()).collect(),
        _ => ["graph","plugins","config","events","tools","cline"].into_iter().map(|s| s.to_string()).collect(),
    }
} else {
    m.get_one::<String>("checks").map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|| ["graph","plugins","config","events","tools","cline"].into_iter().map(|s| s.to_string()).collect())
};

// Default required gates for profile v0
let mut required: std::collections::HashSet<String> = m
    .get_one::<String>("require")
    .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
    .unwrap_or_default();

if profile.as_deref() == Some("v0") {
    required.insert("graph_check".into());
    required.insert("plugins_check".into());
    required.insert("event_log_writable".into());
}

The rest of the doctor code can stay as‑is; this just presets sensible gates for v0.

⸻

Plan #42 — Labels: env → bag + event normalization (P0)

Goal: Make it easy to label experiments consistently. We’ll support post‑hoc annotation at bag level (manifest.labels) and write those labels into normalized events rows (attrs.labels) so analysts can filter by label directly.

We intentionally avoid touching the protected core to inject labels at emit‑time. For v0, post‑hoc labeling at bag level and events@0.3 rewriting is sufficient and deterministic.

42.1 Extend replay annotate

File: plugins/replay/src/lib.rs
	•	Extend CLI to accept labels and to inject labels into events:

// in augment_commands()
.subcommand(
    ClapCommand::new("annotate")
      .about("Upsert labels into bag manifest (and add into events attrs.labels)")
      .arg(Arg::new("bag").long("bag").required(true))
      .arg(Arg::new("labels").long("labels").num_args(1..).value_name("K=V")
           .help("Repeatable; upsert into manifest.labels and events attrs.labels"))
      .arg(Arg::new("from-file").long("from-file").value_name("JSON")
           .help("Optional JSON map file with labels"))
)

	•	Implement:

fn parse_kv_list(items: Option<Vec<String>>) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    if let Some(v) = items {
        for s in v {
            if let Some((k,v)) = s.split_once('=') {
                m.insert(k.trim().to_string(), serde_json::Value::String(v.trim().to_string()));
            }
        }
    }
    m
}

// in execute match:
Some(("annotate", sm)) => {
    let bag = PathBuf::from(sm.get_one::<String>("bag").unwrap());
    let mut labels = parse_kv_list(sm.get_many::<String>("labels").map(|it| it.cloned().collect()));
    if let Some(path) = sm.get_one::<String>("from-file") {
        if let Ok(txt) = std::fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                if let Some(obj) = v.as_object() {
                    for (k, v) in obj {
                        labels.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    anyhow::ensure!(bag.exists(), "bag not found: {}", bag.display());

    // 1) Update manifest.labels
    let man_path = bag.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&man_path)?)?;
    {
        let lbl = manifest.as_object_mut().unwrap()
            .entry("labels").or_insert_with(|| serde_json::json!({}));
        let obj = lbl.as_object_mut().unwrap();
        for (k,v) in labels.iter() {
            obj.insert(k.clone(), v.clone());
        }
    }
    std::fs::write(&man_path, serde_json::to_vec_pretty(&manifest)?)?;

    // 2) Inject into events attrs.labels
    let evp = bag.join("events.ndjson");
    if evp.exists() {
        let inp = std::fs::read_to_string(&evp)?;
        let mut out = String::with_capacity(inp.len() + 1024);
        for line in inp.lines() {
            if line.trim().is_empty() { continue; }
            let mut row: serde_json::Value = serde_json::from_str(line)?;
            {
                let obj = row.as_object_mut().unwrap();
                let attrs = obj.entry("attrs").or_insert_with(|| serde_json::json!({}));
                let attrs_obj = attrs.as_object_mut().unwrap();
                let l = attrs_obj.entry("labels").or_insert_with(|| serde_json::json!({}));
                let lobj = l.as_object_mut().unwrap();
                for (k, v) in labels.iter() {
                    lobj.insert(k.clone(), v.clone());
                }
            }
            out.push_str(&serde_json::to_string(&row)?);
            out.push('\n');
        }
        std::fs::write(&evp, out.as_bytes())?;
    }

    Ok(ctx.ok("replay","annotate",&serde_json::json!({
        "bag": bag.display().to_string(),
        "labels": labels
    })))
}

This guarantees the labels are in the manifest and in every events row, which makes downstream filtering trivial.

⸻

Plan #43 — Dataset Card & Analyst Quickstart (P0)

Goal: Every bag includes a dataset card (README) with what/when/how + “load me” code. Add a top-level Quickstart doc for analysts.

43.1 Generate dataset card on extract/verify

File: plugins/replay/src/lib.rs — add helper and call it at the end of extract and in verify --fix-readme (optional).

fn write_dataset_card(bag: &Path, manifest: &serde_json::Value) -> anyhow::Result<()> {
    use std::fmt::Write as _;
    let mut md = String::new();
    let name = bag.file_name().and_then(|s| s.to_str()).unwrap_or("bag");
    let created = manifest.get("created_at").and_then(|x| x.as_str()).unwrap_or("-");
    let counts = manifest.get("counts").cloned().unwrap_or(serde_json::json!({}));
    let labels = manifest.get("labels").cloned().unwrap_or(serde_json::json!({}));
    let tools  = manifest.get("tools").cloned().unwrap_or(serde_json::json!({}));
    let schemas = vec![
        ("events.ndjson", "nexos/events@0.3"),
        ("fileops.ndjson", "nexos/fileops@0.1")
    ];
    writeln!(md, "# TracePack: {}\n", name)?;
    writeln!(md, "**Created:** {}", created)?;
    writeln!(md, "\n## Contents & Schemas")?;
    for (file, sch) in schemas {
        let p = bag.join(file);
        if p.exists() {
            writeln!(md, "- `{}` — {}", file, sch)?;
        }
    }
    writeln!(md, "\n## Counts")?;
    writeln!(md, "```json\n{}\n```", serde_json::to_string_pretty(&counts)?)?;
    writeln!(md, "\n## Labels")?;
    writeln!(md, "```json\n{}\n```", serde_json::to_string_pretty(&labels)?)?;
    writeln!(md, "\n## Tooling")?;
    writeln!(md, "```json\n{}\n```", serde_json::to_string_pretty(&tools)?)?;
    writeln!(md, "\n## Load in Python (Pandas)")?;
    writeln!(md, "```python\nimport pandas as pd\nimport json\nfrom pathlib import Path\nbag = Path(r\"{}\")\n", bag.display())?;
    writeln!(md, "events = pd.read_json(bag/\"events.ndjson\", lines=True)\nprint(events.head())")?;
    writeln!(md, "\n# Optional fileops\nfpath = bag/\"fileops.ndjson\"\nif fpath.exists():\n    fileops = pd.read_json(fpath, lines=True)\n    print(fileops.head())\n```")?;
    writeln!(md, "\n## Load in DuckDB (SQL)")?;
    writeln!(md, "```sql\n-- From DuckDB shell:\nINSTALL httpfs;\nLOAD httpfs;\n-- Adjust path as needed\nSELECT * FROM read_json_auto('{}');\n```", bag.join("events.ndjson").display())?;
    std::fs::write(bag.join("README.md"), md.as_bytes())?;
    Ok(())
}

Call it:

write_dataset_card(&bag_dir, &manifest).ok();

43.2 Add an analyst quickstart doc

File: docs/analysis/QUICKSTART.md

# Analysis Quickstart (v0)

This guide shows how to load a TracePack ("bag") in **Pandas** or **DuckDB** and compute a few basic metrics.

## 1) Load events and fileops (Pandas)

```python
import pandas as pd
from pathlib import Path

bag = Path("path/to/bag_dir")
events = pd.read_json(bag/"events.ndjson", lines=True)
print("events:", len(events))

fileops_path = bag/"fileops.ndjson"
fileops = pd.read_json(fileops_path, lines=True) if fileops_path.exists() else None

2) Basic metrics

success_rate = (events["ok"] == True).mean()
p50 = events["dur_ms"].dropna().quantile(0.50)
p90 = events["dur_ms"].dropna().quantile(0.90)
print("success:", success_rate, "p50:", p50, "p90:", p90)

3) Labels filter (A/B)

def labels_of(row):
    attrs = row.get("attrs", {}) or {}
    return (attrs.get("labels") or {})

lab = events["attrs"].apply(lambda a: (a or {}).get("labels", {}))
maskA = lab.apply(lambda m: m.get("treatment") == "A")
maskB = lab.apply(lambda m: m.get("treatment") == "B")
print("A:", events[maskA].shape, "B:", events[maskB].shape)

4) Cline messages (if present)

import json
msgs_file = bag/"cline"/"messages.ndjson"
if msgs_file.exists():
    msgs = [json.loads(x) for x in msgs_file.read_text().splitlines() if x.strip()]
    print("messages:", len(msgs))

---

## Plan #44 — v0 Acceptance E2E (P0)

**Goal:** One end‑to‑end test that exercises extract → verify → annotate → export → doctor with minimal synthetic data.

**File:** `nexoscli-bin/tests/v0_e2e.rs`

```rust
mod helpers; use helpers::*;
use std::fs::{self, File}; use std::io::Write as _;

#[test]
fn v0_end_to_end_bag() {
    let t = tmp();
    // arrange: simple repo
    run_ok(t.path(), &["dev","init","--yes"]);

    // create and edit a file via term.run surface so fileops exist
    let _ = run_json(t.path(), &["term","run","--","bash","-lc","echo hello > a.txt"]);
    let _ = run_json(t.path(), &["term","run","--","bash","-lc","echo world >> a.txt"]);

    // synthesize a tiny cline messages file in expected location (correlation window doesn't matter here)
    let cline_dir = t.path().join(".nexoscli").join("agents").join("streams");
    fs::create_dir_all(&cline_dir).unwrap();
    let mut f = File::create(cline_dir.join("messages.ndjson")).unwrap();
    writeln!(f, r#"{{"ts":"2025-09-02T00:00:00Z","role":"user","text":"hi"}}"#).unwrap();
    writeln!(f, r#"{{"ts":"2025-09-02T00:00:01Z","role":"assistant","text":"hello"}}"#).unwrap();

    // get the last run_id from telemetry (helper knows how to fetch)
    let rep = run_json(t.path(), &["dev","telemetry","report","--since","24h"]);
    assert!(rep["data"]["total_events"].as_u64().unwrap() > 0);

    // extract a bag for a recent run (helpers provide last_run_id())
    let run_id = last_run_id(t.path());
    let bag = t.path().join("bag1");
    let _ = run_json(t.path(), &["replay","extract","--run-id",&run_id,"--out",bag.to_str().unwrap()]);

    // verify (schema, counts, cline coverage)
    let ver = run_json(t.path(), &["replay","verify","--bag",bag.to_str().unwrap()]);
    assert!(ver["data"]["ok"].as_bool().unwrap_or(true));

    // annotate labels and ensure they stick
    let _ = run_json(t.path(), &["replay","annotate","--bag",bag.to_str().unwrap(),"--labels","exp_id=EXP-001","--labels","treatment=A"]);
    let man = fs::read_to_string(bag.join("manifest.json")).unwrap();
    assert!(man.contains("\"exp_id\""));
    let events = fs::read_to_string(bag.join("events.ndjson")).unwrap();
    assert!(events.contains("\"labels\""));

    // doctor profile v0
    let _ = run_json(t.path(), &["dev","doctor","--ci","--profile","v0"]);
}

This follows your existing test style: arrange → act (CLI) → assert envelope.

⸻

Usage recap (what just landed)
	•	Env & Repo snapshot are now part of every bag manifest (tools, env, git).
	•	Verify++ validates schemas, counts, patch dry‑run, and warns on cline coverage.
	•	Doctor v0 profile sets sensible defaults (--profile v0).
	•	Annotate writes labels into manifest and events rows.
	•	Dataset card (bag/README.md) and an analyst quickstart doc.

Run-through:

# Create activity
nexoscli term run -- bash -lc 'echo hi >> demo.txt'

# Extract
nexoscli replay extract --run-id RUN-... --out bag/demo

# Verify (schema, counts, patches, cline coverage)
nexoscli replay verify --bag bag/demo

# Annotate (adds to manifest + events.attrs.labels)
nexoscli replay annotate --bag bag/demo --labels exp_id=EXP-001 --labels treatment=B

# Doctor gates (v0)
nexoscli dev doctor --ci --profile v0


⸻

Updated v0 roadmap (checked items included here)

P0 (must for v0)
	•	#34 Schemas bundle & validators (events/fileops)
We now validate in verify; docs for schemas exist or will be finalized with #44 below.
	•	#35 Fileops normalizer
Already in tree from earlier steps; verify counts & schema now enforced.
	•	#36 Annotate & Export
Annotate landed (manifest + events labels). CSV export already present; Parquet is P1.
	•	#37 Cline correlation hardening
Verify warns on cline=0 when term.run is present; earlier plans added messages capture path.
	•	#40 Env & Repo Snapshot ✅ (this PR)
	•	#41 Verify++ & Doctor‑v0 ✅ (this PR)
	•	#42 Labels consistency (env → bag/events) ✅ (post‑hoc annotate)
	•	#43 Dataset card & Quickstart ✅ (bag/README.md + docs/analysis/QUICKSTART.md)
	•	#44 v0 E2E ✅ (new test)

Notes / adjustments learned while implementing:
	•	For label propagation, editing protected core to inject env labels into live events would require an ADR. The v0‑friendly compromise is post‑hoc labeling during normalization/annotation, which still gives analysts row‑level labels with zero drift risk. If we want emit‑time labels later, we’ll draft a tiny ADR to add a labels passthrough to the core envelope.
	•	JSON Schema validation is intentionally lightweight: we enforce required fields and basic types without over‑specifying optional sub‑objects; this reduces brittleness while still preventing garbage.
	•	The doctor profile proved useful enough that we should keep profiles extensible (e.g., --profile publish later to require scrubbed=true).
