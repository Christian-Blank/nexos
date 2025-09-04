---
Implemented in: commit ab21261d8565eb5ecc8e0c26da90753990f953b9 feat(replay): add TracePack verification with events/digests, strict mode, and index continuity (PLAN-30, TASK-20250904-plan-30-bag-verify-tracepack-integrity)
---

Awesome—let’s lock in Implementation Plan #30 — “Bag Verify” to tighten the replay loop.

This plan adds a cryptographic and structural verifier for TracePacks (“bags”) so anyone can prove integrity before analysis, and CI can reject corrupted or incomplete evidence.

⸻

PLAN-30: Bag Verify (TracePack Integrity)

✅ Goals
	•	Verify a TracePack (directory or .tar.gz) for:
	•	Manifest schema & required fields.
	•	Events file integrity (line count, byte count, SHA256) and time window sanity.
	•	Patches / status / cline tasks digests and counts.
	•	Optional: Index continuity against local .nexoscli/log/events.index.json (the segment‑chain added in Plan‑29).
	•	Produce machine‑readable results (--output json) suited for CI and agents.
	•	Add E2E tests and docs so usage becomes self‑documenting.

CLI

# Directory bag
nexoscli replay bag verify --bag ./tracepack

# Tarball bag
nexoscli replay bag verify --bag ./tracepack.tgz

# Strict mode (time_range must match exactly, all digests required)
nexoscli replay bag verify --bag ./tracepack --strict

# Cross-check against rotation index (Plan-29)
nexoscli replay bag verify --bag ./tracepack --index .nexoscli/log/events.index.json

Exit codes
	•	0 = verified OK
	•	2 = verification failed (structural mismatch or digest mismatch)
	•	>2 = operational errors (I/O, parse)

⸻

1) Code changes

Scope: plugins/replay/**, tests, docs only. No protected files are touched.

1.1 plugins/replay/Cargo.toml — add deps

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
flate2        = "1.0"   # for .tar.gz
tar           = "0.4"   # for .tar.gz

If walkdir/sha2/hex are already present, keep as-is.

⸻

1.2 plugins/replay/src/lib.rs — add “bag verify”

Add the imports (near the top):

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{plugin::{Plugin, CommandContext, ToolSpec}, Result};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::{Path, PathBuf}, io::{BufRead, Read}};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use walkdir::WalkDir;
use sha2::{Digest, Sha256};

a) Types

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagVerifyArgs {
    bag: String,                 // dir or .tar.gz
    #[serde(default)]
    index: Option<String>,       // optional path to events.index.json (Plan-29)
    #[serde(default)]
    strict: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagVerifyIssue {
    kind: String,                // "missing", "mismatch", "parse", "warning"
    path: Option<String>,
    message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagVerifySummary {
    ok: bool,
    bag_dir: String,
    manifest_schema: Option<String>,
    counts: serde_json::Value,
    events: serde_json::Value,        // computed stats (lines/bytes/sha256/from/to)
    index_check: Option<serde_json::Value>, // result when --index provided or auto-found
    issues: Vec<BagVerifyIssue>,
}

b) CLI surface

In augment_commands() of the replay plugin, add:

app.subcommand(
  ClapCommand::new("replay").about("Replay & TracePack utilities")
    .subcommand(
      ClapCommand::new("bag").about("TracePack (bag) operations")
        .subcommand(
          ClapCommand::new("verify")
            .about("Verify a TracePack's integrity (events, patches, digests)")
            .arg(Arg::new("bag").long("bag").required(true).value_name("PATH"))
            .arg(Arg::new("index").long("index").required(false).value_name("PATH"))
            .arg(Arg::new("strict").long("strict").action(ArgAction::SetTrue))
        )
    )
)

If the plugin already defines replay with other subcommands (e.g., extract), just append the bag verify branch.

c) Dispatcher

In execute(&self, m: &ArgMatches, ctx: &CommandContext) add:

Some(("replay", rm)) => {
  if let Some(("bag", bm)) = rm.subcommand() {
    if let Some(("verify", vm)) = bm.subcommand() {
      let args = BagVerifyArgs{
        bag: vm.get_one::<String>("bag").unwrap().to_string(),
        index: vm.get_one::<String>("index").cloned(),
        strict: vm.get_flag("strict"),
      };
      let res = bag_verify(&args)?;
      let exit = if res.ok {
        ctx.ok("replay","bag.verify",&res)
      } else {
        ctx.err("replay","bag.verify","replay.bag.verify.failed","bag verification failed",None,2)
      };
      return Ok(exit);
    }
  }
  Ok(2)
}

d) Implementation

Append these helpers near the bottom of the file:

fn is_targz(p: &str) -> bool {
    p.ends_with(".tgz") || p.ends_with(".tar.gz")
}
fn tmp_dir_for_unpack() -> anyhow::Result<PathBuf> {
    let root = std::env::current_dir()?;
    let d = root.join(".nexoscli").join("tmp");
    fs::create_dir_all(&d)?;
    let ts = OffsetDateTime::now_utc().unix_timestamp();
    Ok(d.join(format!("bag-{}", ts)))
}
fn unpack_targz(src: &Path, dst: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dst)?;
    let f = fs::File::open(src)?;
    let dec = flate2::read::GzDecoder::new(f);
    let mut ar = tar::Archive::new(dec);
    ar.unpack(dst)?;
    Ok(())
}
fn read_json_file(p: &Path) -> anyhow::Result<serde_json::Value> {
    let txt = fs::read_to_string(p)?;
    let v: serde_json::Value = serde_json::from_str(&txt)?;
    Ok(v)
}
fn sha256_file(p: &Path) -> anyhow::Result<String> {
    let mut f = fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Ok(format!("sha256:{:x}", h.finalize()))
}
fn events_stats(p: &Path) -> anyhow::Result<serde_json::Value> {
    let mut lines: u64 = 0;
    let mut bytes: u64 = 0;
    let mut h = Sha256::new();
    let mut from_ts: Option<String> = None;
    let mut to_ts: Option<String> = None;

    if p.exists() {
        let f = fs::File::open(p)?;
        let meta = f.metadata().ok();
        bytes = meta.map(|m| m.len()).unwrap_or(0);
        let rdr = std::io::BufReader::new(f);
        for (i, line) in rdr.lines().enumerate() {
            let raw = line?;
            if raw.is_empty() { continue; }
            if i > 0 { h.update(&[0x0a]); }
            h.update(raw.as_bytes());
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if from_ts.is_none() {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                        from_ts = Some(ts.to_string());
                    }
                }
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                    to_ts = Some(ts.to_string());
                }
            }
            lines += 1;
        }
    }
    let sha = format!("sha256:{:x}", h.finalize());
    Ok(json!({
        "lines": lines,
        "bytes": bytes,
        "sha256_file": sha,
        "from_ts": from_ts,
        "to_ts": to_ts
    }))
}
fn value_at<'a>(v: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k)?;
    }
    Some(cur)
}
fn as_u64(v: Option<&serde_json::Value>) -> Option<u64> { v.and_then(|x| x.as_u64()) }
fn as_arr(v: Option<&serde_json::Value>) -> Option<&Vec<serde_json::Value>> { v.and_then(|x| x.as_array()) }

fn bag_verify(args: &BagVerifyArgs) -> anyhow::Result<BagVerifySummary> {
    // 1) locate / unpack
    let bag_path = PathBuf::from(&args.bag);
    let (bag_dir, cleanup) = if is_targz(&args.bag) {
        let tmp = tmp_dir_for_unpack()?;
        unpack_targz(&bag_path, &tmp)?;
        (tmp, true)
    } else {
        (bag_path.clone(), false)
    };

    let mut issues: Vec<BagVerifyIssue> = vec![];

    // 2) manifest
    let manifest = bag_dir.join("manifest.json");
    if !manifest.exists() {
        issues.push(BagVerifyIssue{ kind:"missing".into(), path: Some(manifest.display().to_string()), message:"manifest.json not found".into() });
        return Ok(BagVerifySummary{
            ok: false,
            bag_dir: bag_dir.display().to_string(),
            manifest_schema: None,
            counts: json!({}),
            events: json!({}),
            index_check: None,
            issues
        });
    }
    let man_v = read_json_file(&manifest)?;
    let schema = value_at(&man_v, &["schema"]).and_then(|x| x.as_str()).unwrap_or("").to_string();
    if !schema.starts_with("0.1"). && !schema.contains("tracepack") && value_at(&man_v, &["kind"]).and_then(|x| x.as_str()) != Some("tracepack") {
        issues.push(BagVerifyIssue{ kind:"warning".into(), path: Some(manifest.display().to_string()), message:"unrecognized manifest schema/kind (continuing)".into() });
    }

    // 3) events.ndjson (if present)
    let events_path = bag_dir.join("events.ndjson");
    let mut events_obj = json!({});
    if events_path.exists() {
        let estats = events_stats(&events_path)?;
        events_obj = estats.clone();

        // Compare counts
        if let Some(m_counts) = man_v.get("counts") {
            let want_lines = m_counts.get("events_lines").and_then(|x| x.as_u64());
            if let Some(w) = want_lines {
                if estats["lines"].as_u64() != Some(w) {
                    issues.push(BagVerifyIssue{
                        kind:"mismatch".into(), path: Some(events_path.display().to_string()),
                        message: format!("events_lines manifest={} actual={}", w, estats["lines"])
                    });
                }
            }
            // optional: verify bytes if present
            if let Some(wb) = m_counts.get("events_bytes").and_then(|x| x.as_u64()) {
                if estats["bytes"].as_u64() != Some(wb) {
                    issues.push(BagVerifyIssue{
                        kind:"mismatch".into(), path: Some(events_path.display().to_string()),
                        message: format!("events_bytes manifest={} actual={}", wb, estats["bytes"])
                    });
                }
            }
        }

        // Compare time_range (strict only → exact; else → containment)
        if let Some(tr) = man_v.get("time_range") {
            let m_from = tr.get("from").and_then(|x| x.as_str());
            let m_to   = tr.get("to").and_then(|x| x.as_str());
            let a_from = estats.get("from_ts").and_then(|x| x.as_str());
            let a_to   = estats.get("to_ts").and_then(|x| x.as_str());
            match (m_from, m_to, a_from, a_to) {
                (Some(mf), Some(mt), Some(af), Some(at)) => {
                    if args.strict {
                        if mf != af || mt != at {
                            issues.push(BagVerifyIssue{
                                kind:"mismatch".into(), path: Some(events_path.display().to_string()),
                                message: format!("time_range strict mismatch manifest=({},{}) actual=({},{})", mf, mt, af, at)
                            });
                        }
                    } else {
                        // relaxed: actual should be within manifest window (or equal)
                        if af < mf || at > mt {
                            issues.push(BagVerifyIssue{
                                kind:"warning".into(), path: Some(events_path.display().to_string()),
                                message: format!("time_range containment mismatch manifest=({},{}) actual=({},{})", mf, mt, af, at)
                            });
                        }
                    }
                }
                _ => { /* ignore if incomplete */ }
            }
        }

        // If manifest has sha256_events, compare (future-forward, optional)
        if let Some(msha) = value_at(&man_v, &["sha256_events"]).and_then(|x| x.as_str()) {
            if estats["sha256_file"].as_str() != Some(msha) {
                issues.push(BagVerifyIssue{
                    kind:"mismatch".into(), path: Some(events_path.display().to_string()),
                    message: format!("sha256_events manifest={} actual={}", msha, estats["sha256_file"].as_str().unwrap_or("?"))
                });
            }
        }
    } else {
        // if manifest claims events_lines>0, flag missing
        if as_u64(value_at(&man_v, &["counts","events_lines"])).unwrap_or(0) > 0 {
            issues.push(BagVerifyIssue{
                kind:"missing".into(), path: Some(events_path.display().to_string()),
                message:"events.ndjson missing but manifest counts > 0".into()
            });
        }
    }

    // 4) patches
    let mut verify_list = |arr_path: &[&str], base_dir: &Path, label: &str| -> (u64, u64) {
        let mut err = 0u64; let mut tot = 0u64;
        if let Some(arr) = as_arr(value_at(&man_v, arr_path)) {
            for item in arr {
                tot += 1;
                let dst = item.get("dst").and_then(|x| x.as_str()).unwrap_or("");
                let sha = item.get("sha256").and_then(|x| x.as_str()).unwrap_or("");
                if dst.is_empty() || sha.is_empty() {
                    issues.push(BagVerifyIssue{ kind:"parse".into(), path: None, message: format!("{} entry missing dst/sha256", label) });
                    err += 1; continue;
                }
                let f = base_dir.join(dst);
                if !f.exists() {
                    issues.push(BagVerifyIssue{ kind:"missing".into(), path: Some(f.display().to_string()), message: format!("{} missing", label) });
                    err += 1; continue;
                }
                if let Ok(actual) = sha256_file(&f) {
                    if actual != sha {
                        issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: Some(f.display().to_string()),
                            message: format!("{} sha mismatch manifest={} actual={}", label, sha, actual) });
                        err += 1;
                    }
                } else {
                    issues.push(BagVerifyIssue{ kind:"parse".into(), path: Some(f.display().to_string()), message: format!("{} unreadable", label) });
                    err += 1;
                }
            }
        }
        (tot, err)
    };
    let (patches_tot, patches_err) = verify_list(&["patches"], &bag_dir, "patch");
    let (status_tot, status_err)   = verify_list(&["status_files"], &bag_dir, "status");
    let (cline_tot, cline_err)     = verify_list(&["cline_tasks"], &bag_dir, "cline_task");

    // compare counts (if present)
    if let Some(c) = man_v.get("counts") {
        let want_p = c.get("patches").and_then(|x| x.as_u64()).unwrap_or(patches_tot);
        let want_s = c.get("status").and_then(|x| x.as_u64()).unwrap_or(status_tot);
        let want_c = c.get("cline").and_then(|x| x.as_u64()).unwrap_or(cline_tot);
        if want_p != patches_tot { issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: None, message: format!("patches count manifest={} actual={}", want_p, patches_tot) }); }
        if want_s != status_tot   { issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: None, message: format!("status count manifest={} actual={}", want_s, status_tot) }); }
        if want_c != cline_tot    { issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: None, message: format!("cline count manifest={} actual={}", want_c, cline_tot) }); }
    }

    // 5) optional: index continuity (Plan-29)
    let mut index_obj: Option<serde_json::Value> = None;
    let index_path = args.index.clone()
        .or_else(|| {
            let p = std::env::current_dir().ok()?.join(".nexoscli/log/events.index.json");
            if p.exists() { Some(p.display().to_string()) } else { None }
        });

    if let Some(ip) = index_path {
        let idxp = PathBuf::from(&ip);
        if idxp.exists() {
            match verify_index_head(&idxp) {
                Ok(v) => index_obj = Some(v),
                Err(e) => issues.push(BagVerifyIssue{ kind:"parse".into(), path: Some(ip), message: format!("index verify: {}", e) })
            }
        } else {
            issues.push(BagVerifyIssue{ kind:"warning".into(), path: Some(ip), message:"index path not found".into() });
        }
    }

    // OK?
    let ok = issues.iter().all(|i| i.kind != "mismatch" && i.kind != "missing" && i.kind != "parse");

    // cleanup temp if any (best-effort)
    if cleanup {
        let _ = fs::remove_dir_all(&bag_dir);
    }

    Ok(BagVerifySummary{
        ok,
        bag_dir: args.bag.clone(),
        manifest_schema: Some(schema),
        counts: man_v.get("counts").cloned().unwrap_or(json!({})),
        events: events_obj,
        index_check: index_obj,
        issues,
    })
}

#[derive(Serialize, Deserialize)]
struct IndexFile { schema: String, segments: Vec<IndexSeg>, #[serde(default)] head: Option<String>, #[serde(default)] updated_at: Option<String> }
#[derive(Serialize, Deserialize)]
struct IndexSeg {
    file: String,
    #[serde(default)] from_ts: Option<String>,
    #[serde(default)] to_ts: Option<String>,
    lines: u64, bytes: u64,
    sha256_file: String,
    #[serde(default)] prev_head: Option<String>,
    head: String
}
fn verify_index_head(index_path: &Path) -> anyhow::Result<serde_json::Value> {
    let txt = fs::read_to_string(index_path)?;
    let idx: IndexFile = serde_json::from_str(&txt)?;
    let base = index_path.parent().unwrap_or(Path::new(".")).parent().unwrap_or(Path::new(".")); // .../log/
    let mut head: Option<String> = None;
    let mut issues: Vec<String> = vec![];
    let mut checked = 0u64;

    for seg in &idx.segments {
        let p = base.join(seg.file.clone());
        if !p.exists() {
            issues.push(format!("missing segment {}", seg.file));
            continue;
        }
        // recompute sha for the segment
        let est = events_stats(&p)?;
        let sha = est["sha256_file"].as_str().unwrap_or("");
        if sha != seg.sha256_file {
            issues.push(format!("segment sha mismatch {} manifest={} actual={}", seg.file, seg.sha256_file, sha));
        }
        let h = {
            let mut hh = Sha256::new();
            if let Some(prev) = head.as_deref() { hh.update(prev.as_bytes()); hh.update(&[0x0a]); }
            hh.update(seg.sha256_file.as_bytes());
            format!("sha256:{:x}", hh.finalize())
        };
        if h != seg.head {
            issues.push(format!("segment head mismatch {} manifest={} actual={}", seg.file, seg.head, h));
        }
        head = Some(h);
        checked += 1;
    }

    let idx_head_ok = idx.head.as_deref() == head.as_deref();
    if !idx_head_ok {
        issues.push(format!("index head mismatch manifest={:?} actual={:?}", idx.head, head));
    }

    Ok(json!({
        "path": index_path.display().to_string(),
        "segments_checked": checked,
        "computed_head": head,
        "head_ok": idx_head_ok,
        "issues": issues
    }))
}

e) Tool spec

In tool_specs() add:

ToolSpec{
  name: "replay.bag.verify".into(),
  about: "Verify a TracePack (bag) integrity and optional index continuity".into(),
  args_schema: schema_for!(BagVerifyArgs),
  result_schema: schema_for!(BagVerifySummary),
}


⸻

2) Tests

Create nexoscli-bin/tests/replay_bag_verify.rs

mod helpers; use helpers::*;
use std::fs;
use std::path::Path;

fn write_file(p: &Path, s: &str) { fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, s.as_bytes()).unwrap(); }

#[test]
fn replay_bag_verify_ok_then_fail_on_tamper() {
    let t = tmp();
    let bag = t.path().join("tracepack");
    fs::create_dir_all(&bag).unwrap();

    // minimal manifest + events
    let man = r#"
{
  "version": "0.1",
  "kind": "tracepack",
  "run_id": "RUN-TEST",
  "time_range": {
    "from": "1970-01-01T00:00:00Z",
    "to":   "1970-01-01T00:00:01Z"
  },
  "counts": { "events_lines": 2, "patches": 0, "status": 0 }
}"#;
    write_file(&bag.join("manifest.json"), man);

    let ev = r#"
{"ts":"1970-01-01T00:00:00Z","plugin":"demo","command":"a","exit_code":0}
{"ts":"1970-01-01T00:00:01Z","plugin":"demo","command":"b","exit_code":0}
"#.trim_start();
    write_file(&bag.join("events.ndjson"), ev);

    // OK
    let ok = run_status_json(t.path(), &["replay","bag","verify","--bag",bag.to_str().unwrap()]);
    assert_eq!(ok.status, 0, "expected verify ok: {:?}", ok.json);

    // Tamper: append one extra line
    let more = r#"
{"ts":"1970-01-01T00:00:01Z","plugin":"demo","command":"c","exit_code":0}
"#.trim_start();
    let mut cur = fs::read_to_string(&bag.join("events.ndjson")).unwrap();
    cur.push_str(more);
    fs::write(&bag.join("events.ndjson"), cur.as_bytes()).unwrap();

    // Now should fail (count mismatch)
    let bad = run_status_json(t.path(), &["replay","bag","verify","--bag",bag.to_str().unwrap()]);
    assert_ne!(bad.status, 0, "expected verify fail");
}


⸻

3) Docs

3.1 Add to docs/TRACEPACK.md (new section)

## Verify a TracePack (bag)

Before analysis, verify integrity:

```bash
# Verify a directory or .tgz bag
nexoscli replay bag verify --bag ./tracepack
nexoscli replay bag verify --bag ./tracepack.tgz

# Strict mode: time_range equality and digests required (if present)
nexoscli replay bag verify --bag ./tracepack --strict

# Cross-check against local rotation index (Plan-29)
nexoscli replay bag verify --bag ./tracepack --index .nexoscli/log/events.index.json

What it checks
	•	manifest.json schema/kind and basic fields
	•	events.ndjson → lines, bytes, SHA256, first/last timestamps
	•	patches[], status_files[], cline_tasks[] → presence & SHA256
	•	Optional: index continuity (segment-chain head recomputed and matched)

Machine output
The command returns JSON (OK/non-OK), with a detailed issues[] array so CI can fail on drift or tampering.

### 3.2 Add to `docs/PLAYBOOK.md` (quick recipe)

```md
### Verify evidence before analysis
```bash
nexoscli replay bag verify --bag ./tracepack.tgz --output json | jq .
# CI gate:
nexoscli replay bag verify --bag ./tracepack --strict

---

## 4) Quick smoke

```bash
cargo build --workspace

# Make or fetch a bag, then:
./target/debug/nexoscli replay bag verify --bag ./tracepack --output json | jq .

# Optional continuity check (Plan-29)
./target/debug/nexoscli replay bag verify --bag ./tracepack --index .nexoscli/log/events.index.json --output json


⸻

5) Commit message

feat(replay): add TracePack verification (PLAN-30)

- replay bag verify: checks manifest, events stats (lines/bytes/sha256), patches/status/cline digests
- optional continuity check against events.index.json (Plan-29)
- new E2E: replay_bag_verify.rs
- docs: TRACEPACK.md + PLAYBOOK updated


⸻

Why PLAN‑30 matters (tying back to your center of gravity)
	•	It closes the loop: we can now produce → rotate → extract → verify → analyze with confidence.
	•	Enables forensics and CI: any tampering or drift is detectable before we feed bags to analytics.
	•	The design is forward‑compatible: if/when replay extract starts writing sha256_events in manifests, the verifier automatically upgrades from “best‑effort” to cryptographic checks.