---
Implemented in: commit 1db7ef034a3223d1b2d3b959f0528d4761ddf81b feat(observability): PLAN‑31 File‑change Topic — opt‑in fs watcher, deterministic run linkage, TracePack inclusion (TASK-20250904-plan-31-file-change-topic)
---

Awesome—moving forward with Implementation Plan #31 — “File‑change Topic”.
This plan gives you an opt‑in file‑system change stream during command runs (especially term run) and ensures those changes land in TracePacks so they’re analyzable next to events, patches, and status.

⸻

PLAN‑31: File‑change Topic (fs watcher + bag inclusion)

Goals
	•	Capture file changes (create/modify/remove/rename) that happen during a run.
	•	Store them as a compact NDJSON stream under .nexoscli/fileops/….
	•	Link the stream to the run via a small event record (term.fs.topic), so replay extract can bundle the fileops into the TracePack (bag).
	•	Zero drift: optional and off by default; no existing contracts change unless flags are used.

⸻

CLI surfaces (minimal + stable)

1) term run (opt‑in)

Add these flags to term run:

--fs-topic                # enable fs watcher (default: off)
--fs-root <PATH>...       # roots to watch; default: cwd
--fs-exclude <GLOB>...    # ignores (glob); default: [.git/**, .nexoscli/**, target/**, node_modules/**]
--fs-hash                 # compute sha256 for Create/Modify (default: off)

Behavior (when enabled):
	•	Starts a watcher just before the child process is spawned.
	•	Writes lines to .nexoscli/fileops/fileops-<ts>-<pid>.ndjson:

{"type":"meta","phase":"start","ts":"...","roots":["."],"exclude":[".git/**", ".nexoscli/**", "target/**", "node_modules/**"]}
{"ts":"...","op":"Create","path":"src/lib.rs","size":1234,"sha256":"sha256:..."} 
{"ts":"...","op":"Modify","path":"README.md","size":902}
{"ts":"...","op":"Rename","old":"a.txt","path":"b.txt"}
{"ts":"...","op":"Remove","path":"b.txt"}
{"type":"meta","phase":"end","ts":"...","total":4}


	•	On completion, emits a small event to events.jsonl:

plugin: "term", command: "fs.topic"
data:   { "file": ".nexoscli/fileops/fileops-…ndjson", "lines": N, "bytes": B, "sha256": "sha256:…", "from_ts":"…","to_ts":"…" }

(This ties the file to the run_id so extraction is deterministic.)

2) replay extract

No new flags required. If a term.fs.topic event is found for the run:
	•	Copies the referenced NDJSON into the bag at fileops/fileops.ndjson.
	•	Adds manifest fields:

"counts": { "fileops_lines": <N>, ... },
"fileops": { "dst": "fileops/fileops.ndjson", "sha256": "sha256:…", "bytes": <B>, "lines": <N>, "from": "…", "to": "…" }



If the event is missing (older runs), extractor will fallback to scanning .nexoscli/fileops/*.ndjson and include the one whose time window overlaps the run (±5s pad).

Plan‑32 (“Bag Explain”) will extend bag verify to validate this new artifact; Plan‑31 focuses on capture + extraction.

⸻

Code changes

Scope fences respected: plugins/term/ and plugins/replay/ + tests + docs.
No edits to protected core files.

1) Add dependencies

plugins/term/Cargo.toml

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow        = { workspace = true }
serde         = { workspace = true, features = ["derive"] }
serde_json    = { workspace = true }
schemars      = { workspace = true }
clap          = { workspace = true }
time          = { workspace = true }
notify        = "6"         # cross-platform FS notifications
globset       = "0.4"       # glob ignores
sha2          = { workspace = true }
hex           = { workspace = true }

plugins/replay/Cargo.toml (if not already present for other steps)

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


⸻

2) Term plugin — watcher & flags

plugins/term/src/lib.rs — add flags to term run:

// inside augment_commands() for term.run subcommand
.arg(Arg::new("fs-topic").long("fs-topic").action(ArgAction::SetTrue)
     .help("Enable file-change topic capture during run"))
.arg(Arg::new("fs-root").long("fs-root").num_args(1..).value_name("PATH")
     .help("Watch roots (repeatable). Defaults to current dir"))
.arg(Arg::new("fs-exclude").long("fs-exclude").num_args(1..).value_name("GLOB")
     .help("Exclude globs; defaults: .git/**, .nexoscli/**, target/**, node_modules/**"))
.arg(Arg::new("fs-hash").long("fs-hash").action(ArgAction::SetTrue)
     .help("Compute sha256 for Create/Modify"))

Add the watcher implementation (put near the bottom, or in a small mod fs_topic;):

use notify::{RecommendedWatcher, Watcher, Config, EventKind, event::{ModifyKind, CreateKind, RemoveKind, RenameMode}};
use globset::{Glob, GlobSetBuilder};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use std::{fs, io::{Write, Read}, path::{Path, PathBuf}, sync::mpsc, thread, time::Duration};

#[derive(Clone)]
struct FsTopicCfg {
    roots: Vec<PathBuf>,
    excludes: Vec<String>,
    hash: bool,
}

struct FsTopicHandle {
    path: PathBuf,
    start_ts: String,
    join: Option<std::thread::JoinHandle<(u64,u64,String,String)>>, // (lines, bytes, from_ts, to_ts)
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
}

fn now_rfc3339() -> String { OffsetDateTime::now_utc().format(&Rfc3339).unwrap() }

fn open_fileops_path() -> anyhow::Result<PathBuf> {
    let base = PathBuf::from(".nexoscli").join("fileops");
    fs::create_dir_all(&base)?;
    let ts = OffsetDateTime::now_utc().unix_timestamp();
    let pid = std::process::id();
    Ok(base.join(format!("fileops-{}-{}.ndjson", ts, pid)))
}

fn sha256_file(p: &Path) -> Option<String> {
    let mut f = fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Some(format!("sha256:{:x}", h.finalize()))
}

fn fs_topic_start(cfg: FsTopicCfg) -> anyhow::Result<FsTopicHandle> {
    let out_path = open_fileops_path()?;
    let (tx_ev, rx_ev) = mpsc::channel();
    let (tx_stop, rx_stop) = mpsc::channel::<()>();

    // Build glob excludes
    let mut gb = GlobSetBuilder::new();
    let mut exclude = cfg.excludes.clone();
    if exclude.is_empty() {
        exclude = vec![".git/**".into(), ".nexoscli/**".into(), "target/**".into(), "node_modules/**".into()];
    }
    for g in &exclude { gb.add(Glob::new(g).unwrap()); }
    let gset = gb.build().unwrap();

    let roots = if cfg.roots.is_empty() { vec![std::env::current_dir()?] } else { cfg.roots.clone() };

    // watcher lives on separate thread and writes NDJSON lines
    let start_ts = now_rfc3339();
    let mut file = fs::OpenOptions::new().create(true).append(true).open(&out_path)?;
    writeln!(file, "{{\"type\":\"meta\",\"phase\":\"start\",\"ts\":\"{}\",\"roots\":{},\"exclude\":{}}}",
        start_ts,
        serde_json::to_string(&roots.iter().map(|p| p.display().to_string()).collect::<Vec<_>>())?,
        serde_json::to_string(&exclude)?)?;

    let join = thread::spawn(move || {
        // open again inside thread for exclusive writer
        let mut f = fs::OpenOptions::new().create(true).append(true).open(&out_path).unwrap();
        let mut watcher = RecommendedWatcher::new(tx_ev, Config::default()).unwrap();
        for r in &roots {
            let _ = watcher.watch(r, notify::RecursiveMode::Recursive);
        }

        let mut lines: u64 = 1; // start meta already written
        let mut bytes: u64 = fs::metadata(&out_path).ok().map(|m| m.len()).unwrap_or(0);
        let mut first_ts: Option<String> = None;
        let mut last_ts: Option<String>  = None;

        // event loop
        loop {
            // stop?
            if let Ok(_) = rx_stop.try_recv() { break; }
            match rx_ev.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(evt)) => {
                    // map kind
                    let ts = now_rfc3339();
                    let mut op = None::<serde_json::Value>;

                    // pick one path; prefer the "path" field; we’ll emit relative
                    let mut rel = None::<String>;
                    let mut old_rel = None::<String>;
                    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

                    let paths: Vec<PathBuf> = evt.paths.clone();
                    // ignore excludes
                    let mut filtered = vec![];
                    for p in &paths {
                        let rp = match p.strip_prefix(&cwd) {
                            Ok(r) => r.to_path_buf(),
                            Err(_) => p.clone(),
                        };
                        if gset.is_match(&rp) { continue; }
                        filtered.push(rp);
                    }
                    if filtered.is_empty() { continue; }

                    match evt.kind {
                        EventKind::Create(CreateKind::Any)|EventKind::Create(CreateKind::File)|EventKind::Create(CreateKind::Folder) => {
                            rel = Some(filtered[0].display().to_string());
                            let mut v = serde_json::Map::new();
                            v.insert("ts".into(), ts.clone().into());
                            v.insert("op".into(), "Create".into());
                            v.insert("path".into(), rel.clone().unwrap().into());
                            if let Ok(meta) = fs::metadata(&filtered[0]) { v.insert("size".into(), meta.len().into()); }
                            if cfg.hash && filtered[0].is_file() {
                                if let Some(s) = sha256_file(&filtered[0]) { v.insert("sha256".into(), s.into()); }
                            }
                            op = Some(serde_json::Value::Object(v));
                        }
                        EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Any) => {
                            rel = Some(filtered[0].display().to_string());
                            let mut v = serde_json::Map::new();
                            v.insert("ts".into(), ts.clone().into());
                            v.insert("op".into(), "Modify".into());
                            v.insert("path".into(), rel.clone().unwrap().into());
                            if let Ok(meta) = fs::metadata(&filtered[0]) { v.insert("size".into(), meta.len().into()); }
                            if cfg.hash && filtered[0].is_file() {
                                if let Some(s) = sha256_file(&filtered[0]) { v.insert("sha256".into(), s.into()); }
                            }
                            op = Some(serde_json::Value::Object(v));
                        }
                        EventKind::Remove(RemoveKind::Any)|EventKind::Remove(RemoveKind::File)|EventKind::Remove(RemoveKind::Folder) => {
                            rel = Some(filtered[0].display().to_string());
                            let mut v = serde_json::Map::new();
                            v.insert("ts".into(), ts.clone().into());
                            v.insert("op".into(), "Remove".into());
                            v.insert("path".into(), rel.clone().unwrap().into());
                            op = Some(serde_json::Value::Object(v));
                        }
                        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) |
                        EventKind::Modify(ModifyKind::Name(RenameMode::From)) |
                        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                            let mut v = serde_json::Map::new();
                            v.insert("ts".into(), ts.clone().into());
                            v.insert("op".into(), "Rename".into());
                            if filtered.len() >= 2 {
                                old_rel = Some(filtered[0].display().to_string());
                                rel = Some(filtered[1].display().to_string());
                                v.insert("old".into(), old_rel.clone().unwrap().into());
                                v.insert("path".into(), rel.clone().unwrap().into());
                            } else {
                                // fall back
                                rel = Some(filtered[0].display().to_string());
                                v.insert("path".into(), rel.clone().unwrap().into());
                            }
                            op = Some(serde_json::Value::Object(v));
                        }
                        _ => { /* ignore noise */ }
                    }

                    if let Some(val) = op {
                        let line = serde_json::to_string(&val).unwrap();
                        let _ = writeln!(f, "{}", line);
                        lines += 1;
                        bytes += (line.len() + 1) as u64;
                        last_ts = Some(ts.clone());
                        if first_ts.is_none() {
                            first_ts = val.get("ts").and_then(|x| x.as_str()).map(|s| s.to_string()).or(Some(ts.clone()));
                        }
                    }
                }
                _ => { /* timeout or stop → loop */ }
            }
        }

        let end_ts = now_rfc3339();
        let trailer = format!("{{\"type\":\"meta\",\"phase\":\"end\",\"ts\":\"{}\",\"total\":{}}}\n", end_ts, lines.saturating_sub(1));
        let _ = f.write_all(trailer.as_bytes());
        lines += 1;
        bytes += trailer.len() as u64;
        (lines, bytes, first_ts.unwrap_or(start_ts.clone()), last_ts.unwrap_or(end_ts))
    });

    Ok(FsTopicHandle{ path: out_path, start_ts, join: Some(join), stop_tx: Some(tx_stop) })
}

fn fs_topic_stop(mut h: FsTopicHandle) -> anyhow::Result<(PathBuf, u64, u64, String, String, String)> {
    if let Some(tx) = h.stop_tx.take() { let _ = tx.send(()); }
    let (lines, bytes, from_ts, to_ts) = h.join.take().unwrap().join().unwrap();
    let sha = sha256_file(&h.path).unwrap_or_else(|| "sha256:0".into());
    Ok((h.path, lines, bytes, sha, from_ts, to_ts))
}

Use the watcher in term run execution path:

// parse flags
let fs_topic = m.get_flag("fs-topic");
let roots: Vec<PathBuf> = m.get_many::<String>("fs-root")
    .map(|vals| vals.map(PathBuf::from).collect()).unwrap_or_else(|| vec![]);
let excludes: Vec<String> = m.get_many::<String>("fs-exclude")
    .map(|vals| vals.cloned().collect()).unwrap_or_else(|| vec![]);
let do_hash = m.get_flag("fs-hash");

let mut fs_handle: Option<FsTopicHandle> = None;
if fs_topic {
    fs_handle = Some(fs_topic_start(FsTopicCfg{ roots, excludes, hash: do_hash })?);
}

// ... run the child command as you already do ...

// after child finishes:
if let Some(h) = fs_handle {
    let (path, lines, bytes, sha, from_ts, to_ts) = fs_topic_stop(h)?;
    // Emit a small event tying the topic file to the run
    let _ = ctx.events.write(
        "term", "fs.topic",
        &serde_json::json!({ "file": path.display().to_string(), "lines": lines, "bytes": bytes, "sha256": sha, "from_ts": from_ts, "to_ts": to_ts }),
        0, None, None, None
    );
}

The ctx.events.write(...) call mirrors the pattern you already use in dev doctor --ci (ping). This guarantees the same run_id is attached, so the extractor can deterministically locate the fileops for a given run.

⸻

3) Replay plugin — include fileops in bags

Add a small helper to read a line+bytes+sha and copy the topic into the bag.

plugins/replay/src/lib.rs (within your existing extract implementation):

fn copy_fileops_for_run(run_id: &str, run_from: &str, run_to: &str, out_dir: &Path) -> anyhow::Result<Option<serde_json::Value>> {
    // Strategy:
    // 1) Prefer explicit “term.fs.topic” event inside the run window -> points to a fileops path.
    // 2) Fallback: scan .nexoscli/fileops/*.ndjson and include one whose [from,to] overlaps [run_from,run_to] (±5s).

    use time::{OffsetDateTime, format_description::well_known::Rfc3339};
    let pad = time::Duration::seconds(5);
    let rf = OffsetDateTime::parse(run_from, &Rfc3339).ok().unwrap_or(OffsetDateTime::UNIX_EPOCH) - pad;
    let rt = OffsetDateTime::parse(run_to,   &Rfc3339).ok().unwrap_or(OffsetDateTime::UNIX_EPOCH) + pad;

    // 1) load events for the run_id (you already have them during extract)
    // Assuming you have a vector `events_in_run: Vec<serde_json::Value>`
    // If not, load .nexoscli/log/events.jsonl and filter by run_id

    let mut file_path: Option<PathBuf> = None;
    if let Some(ev) = events_in_run.iter().find(|e| e.get("plugin").and_then(|x| x.as_str())==Some("term")
                                              && e.get("command").and_then(|x| x.as_str())==Some("fs.topic")) {
        // prefer data.file or attrs.file
        if let Some(f) = ev.get("data").and_then(|d| d.get("file")).and_then(|x| x.as_str())
            .or_else(|| ev.get("attrs").and_then(|d| d.get("file")).and_then(|x| x.as_str())) {
            file_path = Some(PathBuf::from(f));
        }
    }
    // 2) fallback scan
    if file_path.is_none() {
        let dir = PathBuf::from(".nexoscli").join("fileops");
        if dir.exists() {
            for ent in walkdir::WalkDir::new(&dir).min_depth(1).max_depth(1).into_iter().flatten() {
                let p = ent.path().to_path_buf();
                if p.extension().and_then(|x| x.to_str()) != Some("ndjson") { continue; }
                // peek first & last lines for ts
                let txt = std::fs::read_to_string(&p).unwrap_or_default();
                let mut first: Option<OffsetDateTime> = None;
                let mut last: Option<OffsetDateTime> = None;
                for line in txt.lines() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(ts) = v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()) {
                            if first.is_none() { first = Some(ts); }
                            last = Some(ts);
                        }
                    }
                }
                if let (Some(f), Some(l)) = (first, last) {
                    if f <= rt && l >= rf {
                        file_path = Some(p);
                        break;
                    }
                }
            }
        }
    }

    if let Some(src) = file_path {
        let dst_rel = PathBuf::from("fileops").join("fileops.ndjson");
        let dst_abs = out_dir.join(&dst_rel);
        if let Some(parent) = dst_abs.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::copy(&src, &dst_abs)?;

        // compute stats
        let txt = std::fs::read_to_string(&dst_abs).unwrap_or_default();
        let lines = txt.lines().count() as u64;
        let bytes = std::fs::metadata(&dst_abs).map(|m| m.len()).unwrap_or(0);
        let sha = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&txt.as_bytes());
            format!("sha256:{:x}", h.finalize())
        };

        // try to recover window
        let mut from: Option<String> = None; let mut to: Option<String> = None;
        for line in txt.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if from.is_none() {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { from = Some(ts.to_string()); }
                }
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { to = Some(ts.to_string()); }
            }
        }

        let info = serde_json::json!({
            "dst": dst_rel.display().to_string(),
            "sha256": sha,
            "bytes": bytes,
            "lines": lines,
            "from": from,
            "to": to
        });
        return Ok(Some(info));
    }
    Ok(None)
}

Hook that into your existing extract path:
	•	After you compute the run time window [from,to], call copy_fileops_for_run(...).
	•	If Some(info), add:
	•	manifest.counts.fileops_lines = info.lines
	•	manifest.fileops = info
	•	Ensure the file is included inside the tarball/directory.

This is intentionally orthogonal: if Plan‑30’s verifier is present, it will silently ignore fileops until we extend it in Plan‑32.

⸻

Tests

Add two integration tests.

nexoscli-bin/tests/term_fileops.rs

mod helpers; use helpers::*;
use std::{fs, thread, time::Duration};

#[test]
fn term_run_emits_fileops_when_enabled() {
    let t = tmp();
    // simple script that modifies files
    let script = r#"sh -c 'echo hi > a.txt; mv a.txt b.txt; rm b.txt'"#;
    let _ = run_json(t.path(), &["term","run","--fs-topic","--","sh","-c","echo hi > a.txt; mv a.txt b.txt; rm b.txt"]);
    // give watcher a brief moment to flush (usually not needed but safer)
    thread::sleep(Duration::from_millis(100));

    let dir = t.path().join(".nexoscli/fileops");
    assert!(dir.exists());
    let mut found = false;
    for ent in fs::read_dir(&dir).unwrap() {
        let p = ent.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) == Some("ndjson") {
            let body = fs::read_to_string(&p).unwrap();
            // minimal assertions
            assert!(body.contains("\"phase\":\"start\""));
            assert!(body.contains("\"op\":\"Create\"") || body.contains("\"op\":\"Modify\"") || body.contains("\"op\":\"Remove\""));
            found = true;
        }
    }
    assert!(found, "no fileops stream found");
}

nexoscli-bin/tests/replay_extract_fileops.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn extract_includes_fileops() {
    let t = tmp();

    // produce a run with fileops
    let v = run_json(t.path(), &["term","run","--fs-topic","--","sh","-c","echo hi > z.txt; rm z.txt"]);
    let run_id = v["meta"]["run_id"].as_str().unwrap_or("");

    // extract
    let pack = t.path().join("pack");
    let _ = run_json(t.path(), &["replay","extract","--run-id", run_id, "--out", pack.to_str().unwrap()]);
    assert!(pack.join("fileops/fileops.ndjson").exists(), "fileops not included");

    let man = fs::read_to_string(pack.join("manifest.json")).unwrap();
    assert!(man.contains("\"fileops\""));
    assert!(man.contains("\"fileops_lines\""));
}

If your term.run test harness differs, keep the intent: create, rename, remove a file while watcher is active.

⸻

Docs

docs/TRACEPACK.md — add “File‑change Topic” section

## File‑change Topic

When enabled, `term run` can record on‑disk changes during a run:

```bash
nexoscli term run --fs-topic --fs-hash -- \
  sh -c 'echo hi > a.txt && mv a.txt b.txt && rm b.txt'

This writes .nexoscli/fileops/fileops-<ts>-<pid>.ndjson with one JSON line per change.
replay extract automatically includes it into the bag at fileops/fileops.ndjson and
adds counts.fileops_lines and a fileops object with sha256/bytes/lines/time window.

### `docs/OBSERVABILITY.md` — mention as a topic

```md
### File Change Topic (opt‑in)
- Enable with `term run --fs-topic [--fs-root ...] [--fs-exclude ...] [--fs-hash]`.
- A small `term.fs.topic` event ties the topic file to the run.
- Included in TracePacks automatically during `replay extract`.


⸻

Smoke

cargo build --workspace

# Record file changes for a run
./target/debug/nexoscli term run --fs-topic --fs-hash -- \
  sh -c 'echo hi > a.txt && mv a.txt b.txt && rm b.txt'

# Verify the topic file exists
ls .nexoscli/fileops/

# Extract to a bag and check manifest/file
./target/debug/nexoscli replay extract --run-id RUN-... --out ./tracepack
cat tracepack/manifest.json | jq '.counts, .fileops'



⸻

Notes & guardrails
	•	The watcher ignores .git/**, .nexoscli/**, target/**, node_modules/** by default; override with --fs-exclude.
	•	--fs-hash trades CPU for integrity detail on Create/Modify events. Off by default.
	•	This is additive and opt‑in. Existing contracts/tests continue to pass without flags.
