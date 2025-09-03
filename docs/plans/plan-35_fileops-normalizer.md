Below is Implementation Plan #35 — “FileOps normalizer (v0)”.
It generates fileops.ndjson for each run/bag in the canonical schema nexos/fileops@0.1 and wires it into replay extract, so every TracePack includes machine‑readable file operations (create/modify/delete/rename) derived from the captured patches.

This plan builds directly on #34 (schemas + validator).

⸻

What you get
	•	Automatic fileops on extract
nexoscli replay extract --run-id ... --out <dir> now writes <dir>/fileops.ndjson by parsing the extracted patches/seq-*.patch files.
	•	Standalone derivation command
nexoscli replay fileops derive --bag <dir> [--out <path>] to (re)generate fileops.ndjson for a bag directory.
	•	Schema‑compliant output (validates with #34):
One NDJSON line per impacted file section in each patch:

{
  "schema":"nexos/fileops@0.1",
  "ts":"2025-09-03T12:34:56Z",
  "run_id":"RUN-...",
  "op":"modify",              // or create|delete|rename
  "path":"src/lib.rs",        // new path (or current for modify/delete/create)
  "from":"old/name.rs",       // present for rename
  "to":"src/lib.rs",          // present for rename
  "patch":{"sha256":"sha256:...","lines":123},
  "source":"patch"            // provenance note
}

(Top-level allows extra fields per schema’s additionalProperties.)

	•	Docs update (TracePack) to include fileops.ndjson in the standard bag layout.
	•	E2E test that exercises: run → extract → validate (events + fileops).

⸻

Patch 1 — Add module and CLI to replay plugin

Files touched: plugins/replay/src/lib.rs (and one new helper fileops.rs inside the same crate).
This is within your replay scope; tests under nexoscli-bin/tests/**; docs under docs/**.

1) New helper module: patch parser → fileops

plugins/replay/src/fileops.rs (new)

use serde::Serialize;
use std::{fs, path::{Path, PathBuf}};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Debug, Serialize, Clone)]
pub struct FileOpLine {
    pub schema: &'static str,             // "nexos/fileops@0.1"
    #[serde(skip_serializing_if="Option::is_none")]
    pub ts: Option<String>,               // RFC3339
    #[serde(skip_serializing_if="Option::is_none")]
    pub run_id: Option<String>,
    pub op: String,                       // create|modify|delete|rename
    pub path: String,                     // current/new path
    #[serde(skip_serializing_if="Option::is_none")]
    pub from: Option<String>,             // for rename
    #[serde(skip_serializing_if="Option::is_none")]
    pub to: Option<String>,               // for rename
    #[serde(skip_serializing_if="Option::is_none")]
    pub bytes: Option<u64>,               // unknown in v0 (left None)
    #[serde(skip_serializing_if="Option::is_none")]
    pub digest_after: Option<String>,     // unknown in v0
    #[serde(skip_serializing_if="Option::is_none")]
    pub source: Option<String>,           // "patch"
    #[serde(skip_serializing_if="Option::is_none")]
    pub context: Option<serde_json::Value>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub patch: Option<PatchMeta>,
}

#[derive(Debug, Serialize, Clone)]
pub struct PatchMeta {
    pub sha256: String,
    #[serde(skip_serializing_if="Option::is_none")]
    pub lines: Option<u64>,
}

fn sha256(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b);
    format!("sha256:{:x}", h.finalize())
}

/// Parse unified diff content into per-file sections (diff --git a/... b/...)
/// and classify as create|delete|rename|modify using header markers.
pub fn derive_from_patch_bytes(run_id: Option<&str>, default_ts: Option<&str>, patch_bytes: &[u8]) -> Vec<FileOpLine> {
    let text = String::from_utf8_lossy(patch_bytes);
    let mut out = Vec::<FileOpLine>::new();

    // split by "diff --git " boundaries, keep header
    let sep = "\ndiff --git ";
    let mut blocks = Vec::<String>::new();
    if text.starts_with("diff --git ") {
        blocks.push(text.clone().to_string());
    }
    for part in text.split(sep).skip(if text.starts_with("diff --git ") {1} else {0}) {
        blocks.push(format!("diff --git {}", part));
    }

    let digest = sha256(patch_bytes);
    let line_count = text.lines().count() as u64;

    for blk in blocks {
        // find the a/ and b/ paths on the first header line
        // header example: diff --git a/src/lib.rs b/src/lib.rs
        let header_line = blk.lines().next().unwrap_or_default();
        let mut a_path = None::<String>;
        let mut b_path = None::<String>;
        if let Some((_, rest)) = header_line.split_once("diff --git ") {
            let mut parts = rest.split_whitespace();
            if let (Some(a), Some(b)) = (parts.next(), parts.next()) {
                // strip a/ and b/ prefixes
                a_path = a.strip_prefix("a/").map(|s| s.to_string()).or_else(|| Some(a.trim_start_matches("a/").to_string()));
                b_path = b.strip_prefix("b/").map(|s| s.to_string()).or_else(|| Some(b.trim_start_matches("b/").to_string()));
            }
        }

        // classify via presence of special headers
        let is_new = blk.contains("\nnew file mode");
        let is_del = blk.contains("\ndeleted file mode");
        let has_rename_from = blk.contains("\nrename from ");
        let has_rename_to   = blk.contains("\nrename to ");

        let ts = default_ts.map(|s| s.to_string())
            .or_else(|| Some(OffsetDateTime::now_utc().format(&Rfc3339).unwrap()));
        let patch = Some(PatchMeta{ sha256: digest.clone(), lines: Some(line_count) });

        if is_new {
            if let Some(path) = b_path.clone() {
                out.push(FileOpLine{
                    schema: "nexos/fileops@0.1",
                    ts, run_id: run_id.map(|s| s.to_string()),
                    op: "create".into(),
                    path,
                    from: None, to: None, bytes: None, digest_after: None,
                    source: Some("patch".into()),
                    context: None,
                    patch: patch.clone(),
                });
            }
            continue;
        }
        if is_del {
            if let Some(path) = a_path.clone() {
                out.push(FileOpLine{
                    schema: "nexos/fileops@0.1",
                    ts, run_id: run_id.map(|s| s.to_string()),
                    op: "delete".into(),
                    path,
                    from: None, to: None, bytes: None, digest_after: None,
                    source: Some("patch".into()),
                    context: None,
                    patch: patch.clone(),
                });
            }
            continue;
        }
        if has_rename_from && has_rename_to {
            // try to read 'rename from X' / 'rename to Y' headers
            let mut from = None::<String>;
            let mut to = None::<String>;
            for line in blk.lines() {
                if let Some(rest) = line.strip_prefix("rename from ") {
                    from = Some(rest.trim().to_string());
                }
                if let Some(rest) = line.strip_prefix("rename to ") {
                    to = Some(rest.trim().to_string());
                }
            }
            let final_path = to.clone().or(b_path.clone());
            if let Some(path) = final_path {
                out.push(FileOpLine{
                    schema: "nexos/fileops@0.1",
                    ts, run_id: run_id.map(|s| s.to_string()),
                    op: "rename".into(),
                    path,
                    from, to,
                    bytes: None, digest_after: None,
                    source: Some("patch".into()),
                    context: None,
                    patch: patch.clone(),
                });
            }
            continue;
        }
        // default: modify (if path determined)
        if let Some(path) = b_path.clone().or(a_path.clone()) {
            out.push(FileOpLine{
                schema: "nexos/fileops@0.1",
                ts, run_id: run_id.map(|s| s.to_string()),
                op: "modify".into(),
                path,
                from: None, to: None, bytes: None, digest_after: None,
                source: Some("patch".into()),
                context: None,
                patch: patch.clone(),
            });
        }
    }
    out
}

/// Scan bag/patches/*.patch and produce fileops lines.
pub fn derive_for_bag(bag_dir: &Path, run_id: Option<&str>, default_ts: Option<&str>) -> anyhow::Result<Vec<FileOpLine>> {
    let mut lines = Vec::<FileOpLine>::new();
    let patches = bag_dir.join("patches");
    if patches.exists() {
        for ent in walkdir::WalkDir::new(&patches).min_depth(1).max_depth(1) {
            let ent = match ent { Ok(e) => e, Err(_) => continue };
            let p = ent.path();
            if !p.is_file() { continue; }
            if p.extension().and_then(|x| x.to_str()) != Some("patch") { continue; }
            let bytes = fs::read(p)?;
            let mut ll = derive_from_patch_bytes(run_id, default_ts, &bytes);
            lines.append(&mut ll);
        }
    }
    Ok(lines)
}

/// Write NDJSON fileops to path.
pub fn write_fileops_ndjson(path: &Path, rows: &[FileOpLine]) -> anyhow::Result<usize> {
    if let Some(dir) = path.parent() { fs::create_dir_all(dir)?; }
    let mut w = std::io::BufWriter::new(fs::File::create(path)?);
    let mut n = 0usize;
    for r in rows {
        let line = serde_json::to_string(r)?;
        use std::io::Write as _;
        writeln!(w, "{}", line)?;
        n += 1;
    }
    Ok(n)
}

2) Wire the module & new subcommand

Open plugins/replay/src/lib.rs:
	•	Add at top:

mod fileops;     // NEW
mod schemas;     // (from #34)

	•	Extend augment_commands() (inside the replay tree) with a fileops derive subcommand:

.subcommand(
  ClapCommand::new("fileops").about("FileOps utilities")
    .subcommand(
      ClapCommand::new("derive")
        .about("Derive fileops.ndjson from bag patches")
        .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
        .arg(Arg::new("out").long("out").value_name("PATH").help("Defaults to <bag>/fileops.ndjson"))
    )
)

	•	Handle it in execute():

Some(("fileops", fm)) => {
    match fm.subcommand() {
        Some(("derive", dm)) => {
            let bag = dm.get_one::<String>("bag").unwrap();
            let outp = dm.get_one::<String>("out").cloned();
            let bagp = PathBuf::from(bag);
            if !bagp.is_dir() {
                return Ok(ctx.err("replay","fileops.derive","replay.fileops.nodir","bag path is not a directory",None,2));
            }
            // try to read manifest for run_id + ts
            let man = bagp.join("manifest.json");
            let (run_id, ts) = if man.exists() {
                let v: serde_json::Value = serde_json::from_slice(&fs::read(&man)?)?;
                let rid = v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string());
                let ts = v.get("time_range").and_then(|tr| tr.get("to")).and_then(|x| x.as_str()).map(|s| s.to_string());
                (rid, ts)
            } else { (None, None) };
            let lines = fileops::derive_for_bag(&bagp, run_id.as_deref(), ts.as_deref())?;
            let out = outp.unwrap_or_else(|| bagp.join("fileops.ndjson").to_string_lossy().to_string());
            let n = fileops::write_fileops_ndjson(Path::new(&out), &lines)?;
            let exit = ctx.ok("replay","fileops.derive",&serde_json::json!({ "bag": bag, "out": out, "lines": n }));
            Ok(exit)
        }
        _ => Ok(ctx.err("replay","fileops","replay.fileops.unknown","unknown fileops subcommand",None,2)),
    }
}

3) Integrate into replay extract

At the very end of the successful extract path (where you write manifest.json and copy events.ndjson, patches/, status/), append:

// After manifest is written
let man: serde_json::Value = serde_json::from_slice(&fs::read(outdir.join("manifest.json"))?)?;
let run_id = man.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string());
let ts_to = man.get("time_range").and_then(|tr| tr.get("to")).and_then(|x| x.as_str()).map(|s| s.to_string());

let fileops_lines = crate::fileops::derive_for_bag(&outdir, run_id.as_deref(), ts_to.as_deref())?;
let _n = crate::fileops::write_fileops_ndjson(&outdir.join("fileops.ndjson"), &fileops_lines)?;

If your extract implementation already has a post‑step hook/list, add it there.
If patches/ is absent, this simply writes an empty file or nothing; it’s fine either way.
(You can choose to skip writing when lines.is_empty().)

⸻

Patch 2 — Tool specs (optional, for agents)

If your plugin exposes tool specs, append:

ToolSpec{
  name: "replay.fileops.derive".into(),
  about: "Derive fileops.ndjson from bag patches".into(),
  args_schema: schema_for!(serde_json::Value),
  result_schema: schema_for!(serde_json::Value),
},


⸻

Patch 3 — Docs (TracePack)

docs/TRACEPACK.md — add fileops.ndjson to Layout and text:
	•	In Layout, add:

fileops.ndjson        # normalized file operations (nexos/fileops@0.1)

	•	In a new section “File Operations”:

### File Operations

On extract, `nexoscli` generates `fileops.ndjson` from the captured patches:

- One line per impacted file section in the patch set
- `op` ∈ {create, modify, delete, rename}
- Paths from diff headers (`a/...` → `b/...`), with `from`/`to` for renames
- Includes `patch.sha256` and `patch.lines` for traceability

Validate via:

```bash
nexoscli replay validate --bag <dir> --what fileops --output json

---

## Patch 4 — E2E test

**`nexoscli-bin/tests/replay_fileops.rs`** (new)

```rust
mod helpers; use helpers::*;
use std::fs;

#[test]
fn extract_includes_fileops_and_validates() {
    let t = tmp();

    // init repo (helpers::tmp() usually gives a clean git repo; if not, ensure it)
    // create a file and modify via term.run so patches are captured
    run_json(t.path(), &["dev","init","--yes"]);
    // ensure git init & initial commit for deterministic patch capture
    run_json(t.path(), &["term","run","--","bash","-lc","git init && git config user.email test@example.com && git config user.name Test && echo base > a.txt && git add a.txt && git commit -m init"]);
    // now modify + create + rename in one run
    run_json(t.path(), &["term","run","--","bash","-lc","echo more >> a.txt && echo x > b.txt && git add -A && git mv b.txt c.txt || true"]);

    // find the last run id from telemetry report (or from events envelope of last term.run)
    // helper reads last event's run_id (your helpers typically expose meta.run_id on each envelope)
    let rep = run_json(t.path(), &["dev","telemetry","report","--since","24h"]);
    assert!(rep["data"]["total_events"].as_u64().unwrap_or(0) > 0);

    // Extract a bag for the most recent RUN by scanning events.jsonl via replay.extract (your CLI already supports run-id)
    // For tests we often capture run_id straight from previous envelope, but if not, pick last non-empty RUN id from events
    // Here we call extract on the last run written to events.jsonl by the second term.run:
    let events = fs::read_to_string(t.path().join(".nexoscli/log/events.jsonl")).unwrap();
    let last_run = events.lines().rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v.get("plugin").and_then(|x| x.as_str())==Some("term") && v.get("command").and_then(|x| x.as_str())==Some("run"))
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .expect("term.run event present");

    let outdir = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id",&last_run,"--out", outdir.to_str().unwrap()]);

    // fileops produced
    assert!(outdir.join("fileops.ndjson").exists(), "fileops.ndjson not found in bag");

    // validate (events + fileops)
    let val = run_json(t.path(), &["replay","validate","--bag", outdir.to_str().unwrap()]);
    assert!(val["data"]["summary_ok"].as_bool().unwrap_or(false), "validation failed: {:?}", val);

    // basic sanity: at least one fileop line present
    let n = std::fs::read_to_string(outdir.join("fileops.ndjson")).unwrap().lines().count();
    assert!(n >= 1, "expected at least one fileop line");
}

If your helper already returns run_id on each term run envelope, feel free to use that directly instead of scanning events.jsonl.

⸻

Notes & design choices
	•	Patch-based derivation keeps v0 simple and portable. We do not require a git checkout at analysis time; we only interpret the diff headers we already bundle.
	•	Op classification uses standard unified diff markers:
new file mode → create, deleted file mode → delete, rename from/to → rename, else modify.
	•	Multiple files per patch are handled by splitting on diff --git boundaries; each section becomes one fileop line.
	•	Digests: we attach the patch SHA256 and line count for correlation. Content digests (digest_after) remain None in v0 (we can add a content snapshotter later).
	•	Timestamps: we default to the bag time_range.to from manifest.json (if present) to keep lines time‑anchored even when post‑processed; otherwise we use now().
	•	Schema compatibility: #34’s replay validate already knows nexos/fileops@0.1. With this patch, bags validate end‑to‑end.

⸻

Smoke

# Build
cargo build --workspace

# Minimal run with changes
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli term run -- bash -lc 'git init && git config user.email t@e && git config user.name T && echo hi > foo && git add foo && git commit -m init'
./target/debug/nexoscli term run -- bash -lc 'echo more >> foo && git add -A'

# Grab run id from last envelope (or via telemetry/report) and extract
RUN=... # your last term.run
./target/debug/nexoscli replay extract --run-id "$RUN" --out bag

# You should now see:
ls bag
# events.ndjson  manifest.json  patches/  status/  fileops.ndjson

# Validate
./target/debug/nexoscli replay validate --bag bag --output json

Expected: summary_ok = true, events.ok = true, fileops.ok = true.

⸻

Why #35 matters
	•	It turns patch blobs into structured, analyzable signals tied to a run.
	•	It lets analysts measure creation/modification churn, rename waves, and correlate with errors/latency from events.ndjson.
	•	It completes the minimum viable “ROS‑bag” for dev: events + fileops (status and patches remain as provenance artifacts).

If this looks good, the natural follow‑ups are (add to ROADMAP_NEXT.md as suggestions):
	•	#36 – Export helpers (CSV/Parquet) for events & fileops (purely additive; no SDK)
	•	#37 – Term-run content digests (optional) to populate digest_after where cheap, improving tamper evidence.