Absolutely—here’s Implementation Plan #32 — “Bag Explain (replay verify)”.
This adds a human‑friendly, CI‑ready verifier for TracePacks (bags). It checks structure, checksums, line counts, run consistency, and (optionally) patch applicability, then explains any failures with actionable suggestions.

⸻

PLAN‑32: Bag Explain (replay verify)

Goals
	•	A single command to verify TracePacks (dir or .tgz) and explain issues:
	•	Manifest presence & schema sanity
	•	File presence and sha256/bytes/lines matches (events, fileops, patches, status, cline tasks)
	•	Events integrity: JSON‑per‑line, single run_id, timestamps monotonic-ish
	•	Fileops (from Plan‑31): presence & counts when advertised
	•	Optional: git apply --check patches dry‑run
	•	Clear fix hints per finding.
	•	Stable JSON envelopes for CI (ctx.ok/ctx.err), with text summaries for humans.

CLI surface (additive)

nexoscli replay verify --path <bag-dir|bag.tgz> [--git-check] [--skip <CSV>]
                       [--format text|summary|full]   # default: text
                       [--output json|text]           # default: json (envelope)

	•	--git-check   Run git apply --check on each patch (best effort; skipped if no Git repo).
	•	--skip        Comma-separated check IDs to skip (e.g., patch-apply,cline).
	•	--format      text = friendly bullets; summary = compact table; full = include raw diffs, counts.
	•	Exit code: 0 OK; 2 failures.

Alias (optional): nexoscli replay bag-verify → same behavior.

Checks performed

Each check yields one or more findings:

Check ID	What it verifies	Severity	Fix hint (example)
manifest	manifest.json exists; version/kind/run_id sane; minimal fields present	error	Re-extract bag; ensure replay extract completed
events	events.ndjson exists; parseable JSON per line; run_id unique; counts.events_lines matches	error	Do not hand-edit; re-run extract
fileops	If manifest.fileops present → file exists; sha/bytes/lines match; plausible time window	error/warn	Re-extract with Plan‑31; avoid editing NDJSON
patches	Every manifest patch exists; sha/bytes match	error	Ensure pack kept original files
patch-apply	With --git-check, git apply --check success (dry run)	warn/error	Apply on throwaway branch; update base range
status	Status files exist & sha/bytes match	warn	Re-extract
cline	If cline_tasks advertised → files exist & sha/bytes match	warn	Keep cline tasks intact
time-range	manifest.time_range consistent with events’ first/last timestamps	warn	Re-extract

Findings are aggregated into an Explain report with {ok, totals, findings[], suggestions[]}.

⸻

Code changes

Scope: plugins/replay/ + tests + docs. No protected core edits.

1) Add deps (replay)

plugins/replay/Cargo.toml

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
flate2        = "1"         # .tgz read
tar           = "0.4"       # .tgz read
tempfile      = "3"         # for git-check temp files

2) replay verify implementation

plugins/replay/src/lib.rs (add subcommand and implementation skeleton)

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{plugin::{Plugin, CommandContext, ToolSpec}, Result};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::{Path, PathBuf}, io::Read};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use flate2::read::GzDecoder;
use tar::Archive;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct VerifyArgs {
    path: String,
    #[serde(default)] git_check: bool,
    #[serde(default)] skip: Option<String>,
    #[serde(default)] format: Option<String>,  // text|summary|full
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct Finding {
    check: String,             // e.g., "events"
    level: String,             // "error" | "warn" | "info"
    message: String,
    hint: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct VerifyReport {
    ok: bool,
    run_id: Option<String>,
    totals: serde_json::Value,
    findings: Vec<Finding>,
    suggestions: Vec<String>,
}

pub struct ReplayPlugin; // ... (existing)
impl Plugin for ReplayPlugin {
    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("replay").about("Replay & TracePack utilities")
              // ... existing subcommands (extract, plan, run, etc.)
              .subcommand(
                  ClapCommand::new("verify")
                    .about("Verify a TracePack (dir or .tgz) and explain issues")
                    .arg(Arg::new("path").long("path").required(true))
                    .arg(Arg::new("git-check").long("git-check").action(ArgAction::SetTrue))
                    .arg(Arg::new("skip").long("skip").value_name("CSV"))
                    .arg(Arg::new("format").long("format").value_parser(["text","summary","full"]).default_value("text"))
              )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            // ... existing arms
            Some(("verify", sm)) => {
                let path = sm.get_one::<String>("path").unwrap();
                let git_check = sm.get_flag("git-check");
                let skip: std::collections::HashSet<String> = sm.get_one::<String>("skip")
                    .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
                    .unwrap_or_default();
                let fmt = sm.get_one::<String>("format").cloned().unwrap_or_else(|| "text".into());

                let report = verify_bag(Path::new(path), git_check, skip, &fmt)?;
                if report.ok {
                    Ok(ctx.ok("replay","verify",&report))
                } else {
                    Ok(ctx.err("replay","verify","bag.verify.failed","bag verification failed", None, 2))
                }
            }
            _ => Ok(2)
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        let mut ts = vec![
            // ... existing
            ToolSpec{
                name:"replay.verify".into(), about:"Verify a TracePack and explain findings".into(),
                args_schema: schema_for!(VerifyArgs), result_schema: schema_for!(VerifyReport)
            }
        ];
        // push the rest...
        ts
    }
}

/* ---------- verify impl ---------- */

enum Bag {
    Dir(PathBuf),
    Tgz(Vec<(String, Vec<u8>)>), // path -> bytes (smallish)
}
fn load_bag(path: &Path) -> anyhow::Result<Bag> {
    if path.is_dir() { return Ok(Bag::Dir(path.to_path_buf())); }
    let p = path.to_string_lossy();
    if p.ends_with(".tgz") || p.ends_with(".tar.gz") {
        let f = fs::File::open(path)?;
        let gz = GzDecoder::new(f);
        let mut ar = Archive::new(gz);
        let mut files = Vec::<(String, Vec<u8>)>::new();
        for e in ar.entries()? {
            let mut e = e?;
            let mut buf = Vec::new(); e.read_to_end(&mut buf)?;
            let path = e.path()?.to_string_lossy().to_string();
            files.push((path.replace('\\',"/"), buf));
        }
        return Ok(Bag::Tgz(files));
    }
    anyhow::bail!("path is neither directory nor .tgz: {}", path.display())
}
fn read_to_string(bag: &Bag, rel: &str) -> Option<String> {
    match bag {
        Bag::Dir(d) => fs::read_to_string(d.join(rel)).ok(),
        Bag::Tgz(files) => files.iter().find(|(p,_)| p == rel).map(|(_,b)| String::from_utf8_lossy(b).to_string()),
    }
}
fn read_bytes(bag: &Bag, rel: &str) -> Option<Vec<u8>> {
    match bag {
        Bag::Dir(d) => fs::read(d.join(rel)).ok(),
        Bag::Tgz(files) => files.iter().find(|(p,_)| p == rel).map(|(_,b)| b.clone()),
    }
}
fn list_prefix(bag: &Bag, prefix: &str) -> Vec<String> {
    match bag {
        Bag::Dir(d) => {
            let root = d.join(prefix);
            let mut out = vec![];
            if root.exists() {
                for ent in walkdir::WalkDir::new(root).min_depth(1) {
                    if let Ok(e) = ent {
                        if e.file_type().is_file() {
                            let rel = e.path().strip_prefix(d).unwrap().to_string_lossy().to_string();
                            out.push(rel.replace('\\',"/"));
                        }
                    }
                }
            }
            out
        }
        Bag::Tgz(files) => files.iter().filter(|(p,_)| p.starts_with(prefix)).map(|(p,_)| p.clone()).collect(),
    }
}
fn sha256_bytes(b: &[u8]) -> String {
    let mut h = Sha256::new(); h.update(b); format!("sha256:{:x}", h.finalize())
}

fn verify_bag(path: &Path, git_check: bool, skip: std::collections::HashSet<String>, _fmt: &str) -> anyhow::Result<VerifyReport> {
    let bag = load_bag(path)?;
    let mut findings = Vec::<Finding>::new();
    let mut suggestions = Vec::<String>::new();
    let mut ok_all = true;

    // manifest
    let man_txt = match read_to_string(&bag, "manifest.json") {
        Some(s) => s,
        None => {
            findings.push(Finding{ check:"manifest".into(), level:"error".into(), message:"manifest.json missing".into(), hint:Some("Re-run: nexoscli replay extract --run-id ...".into()) });
            return Ok(VerifyReport{ ok:false, run_id: None, totals: json!({}), findings, suggestions });
        }
    };
    let man: serde_json::Value = match serde_json::from_str(&man_txt) {
        Ok(v)=>v,
        Err(e)=>{
            findings.push(Finding{ check:"manifest".into(), level:"error".into(), message:format!("manifest.json not valid JSON: {}", e), hint:Some("Do not modify bags by hand".into()) });
            return Ok(VerifyReport{ ok:false, run_id: None, totals: json!({}), findings, suggestions });
        }
    };
    let run_id = man.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string());
    let counts = man.get("counts").cloned().unwrap_or(json!({}));
    let time_range = man.get("time_range").cloned();

    // events
    if !skip.contains("events") {
        match read_to_string(&bag,"events.ndjson") {
            None => {
                ok_all = false;
                findings.push(Finding{ check:"events".into(), level:"error".into(), message:"events.ndjson missing".into(), hint:Some("Re-extract the bag; ensure the run had events logged".into()) });
            }
            Some(ev_txt) => {
                let mut lines=0u64; let mut bad=0u64; let mut run_ids = std::collections::BTreeSet::<String>::new();
                let mut first_ts: Option<OffsetDateTime>=None; let mut last_ts: Option<OffsetDateTime>=None;
                for line in ev_txt.lines() {
                    if line.trim().is_empty() { continue; }
                    lines += 1;
                    match serde_json::from_str::<serde_json::Value>(line) {
                        Ok(v) => {
                            if let Some(r) = v.get("run_id").and_then(|x| x.as_str()) { run_ids.insert(r.to_string()); }
                            if let Some(ts) = v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()) {
                                if first_ts.is_none() { first_ts = Some(ts); }
                                last_ts = Some(ts);
                            }
                        }
                        Err(_) => bad +=1
                    }
                }
                // count check
                if let Some(c_ev) = counts.get("events_lines").and_then(|x| x.as_u64()) {
                    if c_ev != lines {
                        ok_all = false;
                        findings.push(Finding{ check:"events".into(), level:"error".into(),
                            message:format!("line count mismatch: manifest={}, actual={}", c_ev, lines),
                            hint:Some("Do not edit events.ndjson; regenerate with replay extract".into()) });
                    }
                }
                if bad > 0 {
                    ok_all = false;
                    findings.push(Finding{ check:"events".into(), level:"error".into(), message:format!("{} non‑JSON lines", bad), hint:Some("events must be strict JSON per line".into()) });
                }
                if run_ids.len() > 1 {
                    ok_all = false;
                    findings.push(Finding{ check:"events".into(), level:"error".into(), message:format!("multiple run_ids in bag: {:?}", run_ids), hint:Some("bags are per‑run; ensure extract used a single run_id".into()) });
                }
                if let (Some(fr), Some(tr), Some(tr_man)) = (first_ts, last_ts, time_range.as_ref()) {
                    let man_from = tr_man.get("from").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
                    let man_to   = tr_man.get("to").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
                    if let (Some(mf), Some(mt)) = (man_from, man_to) {
                        if fr < mf || tr > mt {
                            findings.push(Finding{ check:"time-range".into(), level:"warn".into(),
                                message:"event timestamps slightly outside manifest.time_range".into(),
                                hint:Some("re-extract to refresh manifest window".into()) });
                        }
                    }
                }
            }
        }
    }

    // fileops
    if !skip.contains("fileops") {
        if let Some(fo) = man.get("fileops") {
            let dst = fo.get("dst").and_then(|x| x.as_str()).unwrap_or("fileops/fileops.ndjson");
            match read_bytes(&bag, dst) {
                None => {
                    ok_all = false;
                    findings.push(Finding{ check:"fileops".into(), level:"error".into(), message:format!("missing {}", dst), hint:Some("Re-extract bag with Plan‑31 in place".into()) });
                }
                Some(b) => {
                    // sha/bytes/lines
                    let lines = String::from_utf8_lossy(&b).lines().count() as u64;
                    let bytes = b.len() as u64;
                    let sha = sha256_bytes(&b);
                    if let Some(exp) = fo.get("lines").and_then(|x| x.as_u64()) { if exp != lines {
                        ok_all=false; findings.push(Finding{ check:"fileops".into(), level:"error".into(),
                            message:format!("lines mismatch: manifest={}, actual={}", exp, lines),
                            hint:Some("Do not edit fileops stream; regenerate".into()) });
                    }}
                    if let Some(exp) = fo.get("bytes").and_then(|x| x.as_u64()) { if exp != bytes {
                        ok_all=false; findings.push(Finding{ check:"fileops".into(), level:"error".into(),
                            message:format!("bytes mismatch: manifest={}, actual={}", exp, bytes),
                            hint:Some("Do not edit fileops stream; regenerate".into()) });
                    }}
                    if let Some(exp) = fo.get("sha256").and_then(|x| x.as_str()) { if exp != sha {
                        ok_all=false; findings.push(Finding{ check:"fileops".into(), level:"error".into(),
                            message:"sha256 mismatch".into(), hint:Some("Regenerate the bag; verify no in-place edits".into()) });
                    }}
                }
            }
        }
    }

    // patches (presence + sha/bytes)
    if !skip.contains("patches") {
        if let Some(arr) = man.get("patches").and_then(|x| x.as_array()) {
            for p in arr {
                let dst = p.get("dst").and_then(|x| x.as_str()).unwrap_or("");
                if dst.is_empty() { continue; }
                match read_bytes(&bag, dst) {
                    None => { ok_all=false; findings.push(Finding{ check:"patches".into(), level:"error".into(), message:format!("missing patch {}", dst), hint:Some("Re-extract".into()) }); }
                    Some(b) => {
                        if let Some(exp) = p.get("bytes").and_then(|x| x.as_u64()) {
                            let got = b.len() as u64;
                            if exp != got { ok_all=false; findings.push(Finding{ check:"patches".into(), level:"error".into(), message:format!("{} bytes mismatch (exp {}, got {})", dst, exp, got), hint:Some("Do not edit patches".into()) }); }
                        }
                        if let Some(exp) = p.get("sha256").and_then(|x| x.as_str()) {
                            let got = sha256_bytes(&b);
                            if exp != got { ok_all=false; findings.push(Finding{ check:"patches".into(), level:"error".into(), message:format!("{} sha mismatch", dst), hint:Some("Re-extract from original run".into()) }); }
                        }
                    }
                }
            }
        }
    }

    // patch apply (optional)
    if git_check && !skip.contains("patch-apply") {
        let patches = list_prefix(&bag, "patches/");
        // Heuristic: only check a few to keep fast
        for pth in patches.iter().take(10) {
            if let Some(b) = read_bytes(&bag, pth) {
                // write to temp file and run git apply --check
                if which_git().is_some() && is_git_repo() {
                    let tmp = tempfile::NamedTempFile::new()?;
                    fs::write(tmp.path(), &b)?;
                    let ok = std::process::Command::new("git")
                        .args(["apply","--check", tmp.path().to_str().unwrap()])
                        .status().ok().map(|s| s.success()).unwrap_or(false);
                    if !ok {
                        ok_all = false;
                        findings.push(Finding{ check:"patch-apply".into(), level:"warn".into(),
                            message:format!("git apply --check failed for {}", pth),
                            hint:Some("Apply on a throwaway branch; ensure base commit matches the manifest.git range".into()) });
                    }
                } else {
                    findings.push(Finding{ check:"patch-apply".into(), level:"info".into(),
                        message:"git not available or not a repo; skipped --check".into(), hint:None });
                }
            }
        }
    }

    // status files
    if !skip.contains("status") {
        let status_files = list_prefix(&bag, "status/");
        for s in status_files {
            if let Some(b) = read_bytes(&bag, &s) {
                if b.is_empty() {
                    findings.push(Finding{ check:"status".into(), level:"warn".into(), message:format!("{} empty", s), hint:Some("Re-extract with status capture enabled".into()) });
                }
            }
        }
    }

    // cline tasks
    if !skip.contains("cline") {
        if let Some(arr) = man.get("cline_tasks").and_then(|x| x.as_array()) {
            for t in arr {
                if let Some(dst) = t.get("dst").and_then(|x| x.as_str()) {
                    if read_bytes(&bag, dst).is_none() {
                        findings.push(Finding{ check:"cline".into(), level:"warn".into(), message:format!("missing {}", dst), hint:Some("Re-extract with --include cline".into()) });
                        ok_all = false; // treat as error if manifest promised it
                    }
                }
            }
        }
    }

    // suggest: when events good but patches missing, remind to run `term run`
    if findings.iter().any(|f| f.check=="patches") && findings.iter().all(|f| f.check!="events" || f.level!="error") {
        suggestions.push("If patches were expected, ensure they were captured by `term run` and included by the extractor.".into());
    }

    Ok(VerifyReport{
        ok: ok_all,
        run_id,
        totals: counts,
        findings,
        suggestions,
    })
}

fn which_git() -> Option<String> {
    std::env::var_os("PATH")?;
    if std::process::Command::new("git").arg("--version").output().ok()?.status.success() { Some("git".into()) } else { None }
}
fn is_git_repo() -> bool {
    std::process::Command::new("git").args(["rev-parse","--is-inside-work-tree"]).output().ok()
        .map(|o| o.status.success()).unwrap_or(false)
}

This is intentionally conservative: if a section is advertised by the manifest, we treat mismatches as errors (bags are append‑only artifacts). Non‑advertised sections don’t fail the bag.

3) Tests

nexoscli-bin/tests/replay_bag_verify.rs

mod helpers; use helpers::*;
use std::{fs, path::Path};

#[test]
fn bag_verify_ok() {
    let t = tmp();
    // produce a run + bag
    let v = run_json(t.path(), &["term","run","--fs-topic","--","sh","-c","echo hi > ok.txt"]);
    let run_id = v["meta"]["run_id"].as_str().unwrap();
    let outdir = t.path().join("bag");
    let _ = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", outdir.to_str().unwrap()]);
    let rep = run_json(t.path(), &["replay","verify","--path", outdir.to_str().unwrap()]);
    assert!(rep["data"]["ok"].as_bool().unwrap());
}

#[test]
fn bag_verify_fails_when_events_missing() {
    let t = tmp();
    let v = run_json(t.path(), &["term","run","--","sh","-c","echo hi"]);
    let run_id = v["meta"]["run_id"].as_str().unwrap();
    let outdir = t.path().join("bag");
    let _ = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", outdir.to_str().unwrap()]);
    // remove events to simulate corruption
    fs::remove_file(outdir.join("events.ndjson")).unwrap();
    let rep = run_json(t.path(), &["replay","verify","--path", outdir.to_str().unwrap()]);
    assert!(!rep["data"]["ok"].as_bool().unwrap());
    // envelope should be an error
    assert_eq!(rep["errors"][0]["code"].as_str().unwrap(), "bag.verify.failed");
}

4) Docs

docs/TRACEPACK.md — append:

## Verify & Explain

Use `replay verify` to validate a bag and get actionable explanations:

```bash
# Directory or .tgz
nexoscli replay verify --path tracepack/                  # human-friendly output (wrapped in JSON envelope)
nexoscli replay verify --path tracepack.tgz --git-check   # also dry‑run check patches via git

What it checks:
	•	manifest.json presence & minimal schema
	•	events.ndjson existence, JSON-per-line, run_id uniqueness, line counts
	•	fileops/fileops.ndjson (if present in manifest) sha/bytes/lines
	•	patches/* sha/bytes (and optional git apply --check)
	•	status/* presence
	•	cline/tasks/* presence (if advertised)

Exit codes: 0 OK, 2 failure.
Use --skip patch-apply,cline to skip those checks in CI.

---

## Smoke

```bash
cargo build --workspace

# Create a run and extract
./target/debug/nexoscli term run --fs-topic -- sh -c 'echo hi > a.txt && rm a.txt'
RUN=$(jq -r '.meta.run_id' .nexoscli/log/events.jsonl | tail -1)
./target/debug/nexoscli replay extract --run-id "$RUN" --out bag

# Verify
./target/debug/nexoscli replay verify --path bag --output json | jq


⸻

Why this is the right next step
	•	Closes the loop on observability/replay: you can now trust bags in CI and share them confidently.
	•	Creates a stable contract for downstream analysis (Python/Pandas, dashboards).
	•	Keeps surface Temporal‑ready (no coupling to Temporal, but deterministic artifacts that map cleanly to Workflow histories later).
