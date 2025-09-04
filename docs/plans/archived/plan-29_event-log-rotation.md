---
Implemented in: commit db60b1b8ea606f4ca1d40ed42da8e693f58fc458 feat(dev-plugin): add event log rotation + continuity (rotate/status, events-rotation) (TASK-20250904-event-log-rotation)
---
Perfect—keeping momentum and sticking to the observability-first center of gravity.

⸻

Implementation Plan #29 — “Event Log Rotation + Continuity”

Objective. Prevent giant logs, keep evidence durable, and make tamper-evidence continuous across time—without touching protected core files. We add a rotator and a continuity index the doctor can verify. Rotation is explicit (via CLI) so it respects your rules fence.

What we add
	•	New commands
	•	nexoscli dev log rotate — rotate .nexoscli/log/events.jsonl into timestamped segments, compute a per‑segment chain, and update a segment‑chain index.
	•	nexoscli dev log status — quick view of rotation/index status (segment count, head digest).
	•	New CI‑grade check
	•	events-rotation — verifies the index and segments: every segment’s digest re-computes, and the segment‑chain head matches recomputation.

We do not edit the protected event writer. Rotation is a user/agent/CI operation (can be called by a workflow or post‑run hook today; later it can be automated when we introduce non-protected hooks).

⸻

Acceptance criteria
	•	dev log rotate rotates when --max-bytes exceeded, or --daily, or --force, and produces:
	•	.nexoscli/log/segments/events-<stamp>.jsonl
	•	.nexoscli/log/segments/events-<stamp>.chain.json
	•	.nexoscli/log/events.index.json (segment-chain index)
	•	fresh empty .nexoscli/log/events.jsonl
	•	dev log status shows segment count and current segment‑chain head.
	•	dev doctor --ci --checks events-rotation --require events-rotation fails on any index/segment mismatch.
	•	End‑to‑end test covers rotate → verify → tamper → fail.
	•	Documentation updated (how to rotate; the chain model; CI snippets).

⸻

Data shapes

Per‑segment chain manifest (.nexoscli/log/segments/events-<stamp>.chain.json)

{
  "schema": "events_segment_chain.v0.1",
  "file": ".nexoscli/log/segments/events-2025-09-03T12-34-56Z.jsonl",
  "from_ts": "2025-09-03T12:01:02Z",
  "to_ts": "2025-09-03T12:34:56Z",
  "lines": 5132,
  "bytes": 842731,
  "sha256_file": "sha256:..."              // SHA over newline‑separated raw lines
}

Index (.nexoscli/log/events.index.json)

{
  "schema": "events_index.v0.1",
  "segments": [
    {
      "file": ".nexoscli/log/segments/events-2025-09-03T12-34-56Z.jsonl",
      "from_ts": "2025-09-03T12:01:02Z",
      "to_ts": "2025-09-03T12:34:56Z",
      "lines": 5132,
      "bytes": 842731,
      "sha256_file": "sha256:...",
      "prev_head": null,
      "head": "sha256:..."                  // segment‑chain head after including this segment
    }
  ],
  "head": "sha256:...",                     // equals segments.last.head
  "updated_at": "2025-09-03T12:35:04Z"
}

Segment‑chain function (incremental)
We avoid re‑hashing all history by chaining segment digests:

head_0 := sha256( sha256_file(seg_0) )
head_k := sha256( head_{k-1} || "\n" || sha256_file(seg_k) )

If any segment bytes change → sha256_file changes → head changes → CI fails.

⸻

Patches

Scope stays inside plugins/dev/** + tests + docs. No protected files touched.

1) Add CLI surface (plugins/dev/src/lib.rs)

a) Extend augment_commands()

// inside Command::new("dev") tree:
.subcommand(
  Command::new("log").about("Event log utilities")
    .subcommand(
      Command::new("rotate")
        .about("Rotate .nexoscli/log/events.jsonl into timestamped segments with chain manifests")
        .arg(Arg::new("max-bytes").long("max-bytes").value_parser(clap::value_parser!(u64)).default_value("5000000"))
        .arg(Arg::new("daily").long("daily").action(ArgAction::SetTrue))
        .arg(Arg::new("force").long("force").action(ArgAction::SetTrue))
        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
    )
    .subcommand(
      Command::new("status")
        .about("Show rotation/index status")
    )
)

b) Implement handler

Add this new block inside fn execute(...):

// ---------------- dev log ----------------
Some(("log", lm)) => {
    if let Some(("rotate", m)) = lm.subcommand() {
        let max_bytes = *m.get_one::<u64>("max-bytes").unwrap();
        let daily = m.get_flag("daily");
        let force = m.get_flag("force");
        let dry = m.get_flag("dry-run");

        let root = std::env::current_dir()?;
        let log_dir = root.join(".nexoscli").join("log");
        let ev = log_dir.join("events.jsonl");
        let seg_dir = log_dir.join("segments");
        fs::create_dir_all(&seg_dir)?;

        let meta = fs::metadata(&ev).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mut reason = String::new();

        // Compute file stats once (also needed for rotate)
        let stats = compute_event_file_stats(&ev)?;
        let daily_due = daily && stats.from_ts.as_deref().map(|ts| ts.split('T').next().unwrap_or("") != OffsetDateTime::now_utc().format(&Rfc3339).unwrap().split('T').next().unwrap_or("")).unwrap_or(false);

        let should_rotate = if force {
            reason = "force".into(); true
        } else if size >= max_bytes {
            reason = format!("size>={}", max_bytes); true
        } else if daily_due {
            reason = "daily".into(); true
        } else {
            false
        };

        if !should_rotate {
            let exit = ctx.ok("dev","log.rotate.plan",&serde_json::json!({
                "would_rotate": false,
                "size": size,
                "reason": "thresholds not met"
            }));
            return Ok(exit);
        }

        // finalize segment stamp
        let stamp = stats.to_ts.clone().unwrap_or_else(|| OffsetDateTime::now_utc().format(&Rfc3339).unwrap())
            .replace(':','-');

        let seg_file = seg_dir.join(format!("events-{}.jsonl", stamp));
        let seg_chain = seg_dir.join(format!("events-{}.chain.json", stamp));

        if dry {
            let exit = ctx.ok("dev","log.rotate.plan",&serde_json::json!({
                "would_rotate": true,
                "reason": reason,
                "segment": seg_file.display().to_string(),
                "lines": stats.lines,
                "bytes": stats.bytes,
                "sha256_file": stats.sha256_file
            }));
            return Ok(exit);
        }

        // Move file into segment
        if ev.exists() {
            fs::rename(&ev, &seg_file)?;
        } else {
            // create empty placeholder to keep logic consistent
            fs::write(&seg_file, b"")?;
        }

        // Write per-segment chain manifest
        let seg_rec = serde_json::json!({
            "schema": "events_segment_chain.v0.1",
            "file": seg_file.display().to_string(),
            "from_ts": stats.from_ts,
            "to_ts": stats.to_ts,
            "lines": stats.lines,
            "bytes": stats.bytes,
            "sha256_file": stats.sha256_file
        });
        fs::write(&seg_chain, serde_json::to_vec_pretty(&seg_rec)?)?;

        // Update index (segment-chain)
        let mut idx = load_events_index(&log_dir)?;
        let prev_head = idx.head.clone();
        let head = chain_next(prev_head.as_deref(), seg_rec["sha256_file"].as_str().unwrap());
        idx.segments.push(IndexSeg {
            file: seg_file.display().to_string(),
            from_ts: seg_rec["from_ts"].as_str().map(|s| s.to_string()),
            to_ts: seg_rec["to_ts"].as_str().map(|s| s.to_string()),
            lines: stats.lines,
            bytes: stats.bytes,
            sha256_file: stats.sha256_file.clone(),
            prev_head: prev_head.clone(),
            head: head.clone(),
        });
        idx.head = Some(head.clone());
        idx.updated_at = Some(OffsetDateTime::now_utc().format(&Rfc3339).unwrap());
        save_events_index(&log_dir, &idx)?;

        // Recreate fresh events.jsonl
        fs::write(&ev, b"")?;

        let exit = ctx.ok("dev","log.rotate",&serde_json::json!({
            "rotated": true,
            "reason": reason,
            "segment": seg_file.display().to_string(),
            "chain": seg_chain.display().to_string(),
            "index": log_dir.join("events.index.json").display().to_string(),
            "head": idx.head
        }));
        return Ok(exit);
    }
    if let Some(("status", _m)) = lm.subcommand() {
        let root = std::env::current_dir()?;
        let log_dir = root.join(".nexoscli").join("log");
        let idx = load_events_index(&log_dir).unwrap_or_default();
        let segs = idx.segments.len();
        let exit = ctx.ok("dev","log.status",&serde_json::json!({
            "segments": segs,
            "head": idx.head,
            "updated_at": idx.updated_at
        }));
        return Ok(exit);
    }
    Ok(2)
}

c) Support code (helpers + types)

Add below your helper section in plugins/dev/src/lib.rs:

use sha2::{Digest, Sha256};
use std::io::BufRead;

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct IndexFile {
    schema: String,                      // "events_index.v0.1"
    segments: Vec<IndexSeg>,
    #[serde(skip_serializing_if="Option::is_none")]
    head: Option<String>,                // segment-chain head
    #[serde(skip_serializing_if="Option::is_none")]
    updated_at: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
struct IndexSeg {
    file: String,
    #[serde(skip_serializing_if="Option::is_none")] from_ts: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] to_ts: Option<String>,
    lines: u64,
    bytes: u64,
    sha256_file: String,
    #[serde(skip_serializing_if="Option::is_none")] prev_head: Option<String>,
    head: String,
}

fn events_dir_from_log(log_dir: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let ev = log_dir.join("events.jsonl");
    let seg_dir = log_dir.join("segments");
    let idx = log_dir.join("events.index.json");
    (ev, seg_dir, idx)
}

fn compute_event_file_stats(ev: &Path) -> anyhow::Result<ComputeStats> {
    let mut lines: u64 = 0;
    let mut bytes: u64 = 0;
    let mut h = Sha256::new();
    let mut from_ts: Option<String> = None;
    let mut to_ts: Option<String> = None;

    if ev.exists() {
        let f = fs::File::open(ev)?;
        let meta = f.metadata().ok();
        bytes = meta.map(|m| m.len()).unwrap_or(0);
        let rdr = std::io::BufReader::new(f);
        for (i, line) in rdr.lines().enumerate() {
            let raw = line?;
            if raw.trim().is_empty() { continue; }
            if i > 0 { h.update(&[0x0a]); }
            h.update(raw.as_bytes());
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if from_ts.is_none() {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { from_ts = Some(ts.to_string()); }
                }
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { to_ts = Some(ts.to_string()); }
            }
            lines += 1;
        }
    }
    let sha = format!("sha256:{:x}", h.finalize());
    Ok(ComputeStats{ lines, bytes, from_ts, to_ts, sha256_file: sha })
}
#[derive(Debug)]
struct ComputeStats {
    lines: u64,
    bytes: u64,
    from_ts: Option<String>,
    to_ts: Option<String>,
    sha256_file: String,
}

fn load_events_index(log_dir: &Path) -> anyhow::Result<IndexFile> {
    let p = log_dir.join("events.index.json");
    if !p.exists() {
        return Ok(IndexFile{ schema: "events_index.v0.1".into(), segments: vec![], head: None, updated_at: None });
    }
    let txt = fs::read_to_string(&p)?;
    let mut v: IndexFile = serde_json::from_str(&txt).unwrap_or(IndexFile{ schema: "events_index.v0.1".into(), segments: vec![], head: None, updated_at: None });
    if v.schema.is_empty() { v.schema = "events_index.v0.1".into(); }
    Ok(v)
}
fn save_events_index(log_dir: &Path, idx: &IndexFile) -> anyhow::Result<()> {
    let p = log_dir.join("events.index.json");
    if let Some(d) = p.parent() { fs::create_dir_all(d)?; }
    fs::write(p, serde_json::to_vec_pretty(idx)?)?;
    Ok(())
}

fn chain_next(prev: Option<&str>, item: &str) -> String {
    let mut h = Sha256::new();
    if let Some(p) = prev {
        if !p.is_empty() { h.update(p.as_bytes()); h.update(&[0x0a]); }
    }
    h.update(item.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

We reuse sha2 (already added in Plan #28).

⸻

2) Doctor check for rotation (plugins/dev/src/checks.rs)

Append:

// === Rotation index verification ======================================
#[derive(Debug, Serialize, Deserialize)]
struct IndexFile {
    schema: String,
    segments: Vec<IndexSeg>,
    #[serde(default)] head: Option<String>,
    #[serde(default)] updated_at: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct IndexSeg {
    file: String,
    #[serde(default)] from_ts: Option<String>,
    #[serde(default)] to_ts: Option<String>,
    lines: u64,
    bytes: u64,
    sha256_file: String,
    #[serde(default)] prev_head: Option<String>,
    head: String,
}

pub fn check_events_rotation(root: &Path) -> DevCheckOutcome {
    let mut out = DevCheckOutcome{ name:"events-rotation".into(), ok:true, lines:0, errors: vec![], digest: None, file: None };
    let log_dir = root.join(".nexoscli").join("log");
    let idx_path = log_dir.join("events.index.json");
    out.file = Some(idx_path.display().to_string());

    if !idx_path.exists() {
        // No index yet: that's OK; rotation not used.
        return out;
    }

    let txt = match fs::read_to_string(&idx_path) {
        Ok(s) => s,
        Err(e) => { out.ok=false; out.errors.push(format!("read index: {}", e)); return out; }
    };
    let idx: IndexFile = match serde_json::from_str(&txt) {
        Ok(v) => v,
        Err(e) => { out.ok=false; out.errors.push(format!("parse index: {}", e)); return out; }
    };

    // recompute segment-chain head
    let mut head: Option<String> = None;
    for seg in &idx.segments {
        // file exists
        let p = root.join(seg.file.clone());
        if !p.exists() {
            out.ok=false; out.errors.push(format!("missing segment file: {}", seg.file));
            continue;
        }
        // recompute file sha
        let s = super::compute_event_file_stats(&p).ok();
        if let Some(stats) = s {
            if stats.sha256_file != seg.sha256_file {
                out.ok=false; out.errors.push(format!("sha mismatch for {}", seg.file));
            }
            if stats.lines != seg.lines {
                out.ok=false; out.errors.push(format!("line count mismatch for {}", seg.file));
            }
        } else {
            out.ok=false; out.errors.push(format!("cannot compute stats for {}", seg.file));
        }
        head = Some(super::chain_next(head.as_deref(), &seg.sha256_file));
    }
    out.digest = head.clone();

    if idx.head != head {
        out.ok=false; out.errors.push("index head does not match recomputed head".into());
    }

    out.lines = idx.segments.len() as u64;
    out
}

3) Wire the rotation check in dev doctor (CI + local)

In the CI branch (if m.get_flag("ci")), extend want set default to include "events-rotation", then add:

// events-rotation (index & segments continuity)
if want.contains("events-rotation") {
    let outc = checks::check_events_rotation(&cwd);
    checks.push(DoctorCiCheck {
        name: "events-rotation".into(),
        ok: outc.ok,
        detail: format!("segments={} head={:?} errs={}", outc.lines, outc.digest, outc.errors.len()),
        severity: if outc.ok { "info".into() } else { "error".into() },
        fix: Some("Run `nexoscli dev log rotate --max-bytes 5000000 --daily` locally; commit updated events.index.json".into()),
    });
}

In the local branch, add (before computing ready):

let rot = crate::checks::check_events_rotation(&std::env::current_dir()?);
report.insert("events_rotation".into(), serde_json::json!({
  "ok": rot.ok, "segments": rot.lines, "head": rot.digest, "errors": rot.errors
}));
if (!rot.ok) && require.iter().any(|c| *c == "events-rotation") { hard_fail = true; }

And update the --checks help string to include events-rotation.

⸻

4) Tests

nexoscli-bin/tests/dev_log_rotate.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn dev_log_rotate_and_verify() {
    let t = tmp();

    // init + create some events
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    let _ = run_json(t.path(), &["task","new","--title","rotate smoke"]);
    let _ = run_json(t.path(), &["task","start","--task","TASK-19700101-rotate-smoke"]);

    // force tiny threshold to rotate immediately
    let v1 = run_json(t.path(), &["dev","log","rotate","--max-bytes","1"]);
    assert!(v1["errors"].as_array().unwrap().is_empty());
    assert!(t.path().join(".nexoscli/log/events.index.json").exists());

    // CI verify rotation continuity
    let ci = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-rotation","--require","events-rotation"]);
    assert_eq!(ci.status, 0);

    // tamper: change first segment file → verify must fail
    let idx_path = t.path().join(".nexoscli/log/events.index.json");
    let idx: serde_json::Value = serde_json::from_str(&fs::read_to_string(idx_path).unwrap()).unwrap();
    let seg_file = idx["segments"][0]["file"].as_str().unwrap();
    fs::write(t.path().join(seg_file), b"{\"ts\":\"1970-01-01T00:00:00Z\",\"plugin\":\"tamper\",\"command\":\"x\",\"exit_code\":0}\n").unwrap();

    let ci2 = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-rotation","--require","events-rotation"]);
    assert_ne!(ci2.status, 0);
}


⸻

5) Docs

docs/OBSERVABILITY.md — add section

## Log rotation & continuity

Keep `.nexoscli/log/events.jsonl` small and evidence durable.

### Rotate
```bash
# Rotate by size, or once per day
nexoscli dev log rotate --max-bytes 5000000 --daily

Rotation produces:
	•	.nexoscli/log/segments/events-<STAMP>.jsonl — archived segment
	•	.nexoscli/log/segments/events-<STAMP>.chain.json — segment digest/metadata
	•	.nexoscli/log/events.index.json — segment‑chain index with a rolling head

Segment‑chain head is incremental:

head_0 = sha256( sha256_file(seg_0) )
head_k = sha256( head_{k-1} || "\n" || sha256_file(seg_k) )

Status

nexoscli dev log status --output json

CI verification

nexoscli dev doctor --ci \
  --checks events-rotation \
  --require events-rotation

**`docs/PLAYBOOK.md` — add a snippet under “Health Checks”**

```md
# Rotate the event log periodically (local/dev)
nexoscli dev log rotate --max-bytes 5000000 --daily

# Verify rotation index in CI
nexoscli dev doctor --ci --checks events-rotation --require events-rotation


⸻

How to use (quick smoke)

cargo build --workspace

# produce some events
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli task new --title "rotate smoke"
./target/debug/nexoscli task start --task TASK-19700101-rotate-smoke

# rotate by size threshold
./target/debug/nexoscli dev log rotate --max-bytes 1 --output json | jq .

# see status
./target/debug/nexoscli dev log status --output json | jq .

# CI verify (index+segments)
./target/debug/nexoscli dev doctor --ci --checks events-rotation --require events-rotation


⸻
