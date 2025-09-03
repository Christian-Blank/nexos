Fantastic — let’s keep the cadence and stamp this as our next named plan.

⸻

Implementation Plan #28 — “Doctor: Events Schema + Chain‑of‑Custody”

Goal. Make observability tamper‑evident and schema‑sound by default. Add two CI‑grade doctor checks:
	•	events-schema — validate each line of .nexoscli/log/events.jsonl decodes to nexoscli_core::event::Event (no drift).
	•	events-chain — verify a rolling digest over the event log matches .nexoscli/log/events.chain.json.
Local doctor creates/refreshes the chain file; CI doctor verifies only.

This keeps our “bags as evidence” story strong and gives an immediate guardrail before we wire Temporal.

⸻

✅ Acceptance criteria
	•	New checks appear in nexoscli dev doctor --help and can be targeted via --checks.
	•	Local: nexoscli dev doctor --checks events-schema,events-chain creates/refreshes .nexoscli/log/events.chain.json.
	•	CI: nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain fails on:
	•	invalid JSON lines / schema decode errors,
	•	any digest/line‑count mismatch,
	•	missing chain file.
	•	New E2E test dev_doctor_events_chain.rs passes.
	•	Docs updated (how to refresh/verify chain; CI examples).

⸻

✂️ Patch set

1) plugins/dev/Cargo.toml — add dependency

# plugins/dev/Cargo.toml
[dependencies]
sha2 = { workspace = true }

We already use serde_json, time in this crate. sha2 is new for the rolling digest.

⸻

2) plugins/dev/src/checks.rs — add events checks

Append the following (safe to paste at the end of the file):

// === Events schema + chain-of-custody checks ==========================
use std::{fs, io::BufRead, path::Path, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DevCheckOutcome {
    pub name: String,
    pub ok: bool,
    pub lines: u64,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if="Option::is_none")] pub digest: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")] pub file: Option<String>,
}

#[derive(Copy, Clone)]
pub enum ChainMode { Verify, Refresh }

#[derive(Debug, Serialize, Deserialize)]
struct EventsChain {
    schema: &'static str,  // "events_chain.v0.1"
    file: String,
    from_ts: Option<String>,
    to_ts: Option<String>,
    lines: u64,
    sha256: String,
    generated_at: String,
}

const EVENTS_CHAIN_SCHEMA: &str = "events_chain.v0.1";

fn events_file(root: &Path) -> PathBuf {
    root.join(".nexoscli").join("log").join("events.jsonl")
}
fn chain_file(root: &Path) -> PathBuf {
    root.join(".nexoscli").join("log").join("events.chain.json")
}

pub fn check_events_schema(root: &Path, since: Option<OffsetDateTime>) -> DevCheckOutcome {
    let mut out = DevCheckOutcome{ name:"events-schema".into(), ok:true, lines:0, errors: vec![], digest: None, file: None };
    let path = events_file(root);
    out.file = Some(path.display().to_string());
    if !path.exists() { return out; }

    let f = match fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => { out.ok=false; out.errors.push(format!("open: {}", e)); return out; }
    };
    let rdr = std::io::BufReader::new(f);

    for (idx, line) in rdr.lines().enumerate() {
        let raw = match line { Ok(s) => s, Err(e) => { out.ok=false; out.errors.push(format!("read line {}: {}", idx+1, e)); break; } };
        if raw.trim().is_empty() { continue; }
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) => {
                // optional since filter
                if let Some(since_ts) = since {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                        if let Ok(t) = OffsetDateTime::parse(ts, &Rfc3339) {
                            if t < since_ts { continue; }
                        }
                    }
                }
                // hard check: decode into the actual Event type
                if let Err(e) = serde_json::from_value::<nexoscli_core::event::Event>(v) {
                    out.ok=false; out.errors.push(format!("line {}: decode Event failed: {}", idx+1, e));
                }
                out.lines += 1;
            }
            Err(e) => { out.ok=false; out.errors.push(format!("line {}: json parse: {}", idx+1, e)); }
        }
    }
    out
}

pub fn check_events_chain(root: &Path, mode: ChainMode) -> DevCheckOutcome {
    let mut out = DevCheckOutcome{ name:"events-chain".into(), ok:true, lines:0, errors: vec![], digest: None, file: None };
    let epath = events_file(root);
    out.file = Some(epath.display().to_string());
    if !epath.exists() { return out; }

    // rolling hash
    let f = match fs::File::open(&epath) {
        Ok(f) => f,
        Err(e) => { out.ok=false; out.errors.push(format!("open events: {}", e)); return out; }
    };
    let rdr = std::io::BufReader::new(f);
    let mut h = Sha256::new();
    let mut from_ts: Option<String> = None;
    let mut to_ts: Option<String> = None;
    let mut lines = 0u64;

    for (idx, line) in rdr.lines().enumerate() {
        let raw = match line { Ok(s) => s, Err(e) => { out.ok=false; out.errors.push(format!("read line {}: {}", idx+1, e)); break; } };
        if raw.trim().is_empty() { continue; }

        if lines > 0 { h.update(&[0x0a]); }       // include newline separators
        h.update(raw.as_bytes());

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if from_ts.is_none() {
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { from_ts = Some(ts.to_string()); }
            }
            if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { to_ts = Some(ts.to_string()); }
        }
        lines += 1;
    }
    let digest = format!("sha256:{:x}", h.finalize());
    out.digest = Some(digest.clone());
    out.lines = lines;

    // compare with chain file
    let cpath = chain_file(root);
    if cpath.exists() {
        match fs::read_to_string(&cpath) {
            Ok(txt) => {
                match serde_json::from_str::<EventsChain>(&txt) {
                    Ok(rec) => {
                        if rec.sha256 != digest || rec.lines != lines {
                            out.ok = false;
                            out.errors.push("digest or line count mismatch; refresh locally with `nexoscli dev doctor --checks events-chain`".into());
                        }
                    }
                    Err(e) => { out.ok=false; out.errors.push(format!("chain parse: {}", e)); }
                }
            }
            Err(e) => { out.ok=false; out.errors.push(format!("chain read: {}", e)); }
        }
    } else {
        match mode {
            ChainMode::Verify => {
                out.ok = false;
                out.errors.push("chain file missing; create by running local doctor".into());
            }
            ChainMode::Refresh => {
                let rec = EventsChain{
                    schema: EVENTS_CHAIN_SCHEMA,
                    file: epath.display().to_string(),
                    from_ts,
                    to_ts,
                    lines,
                    sha256: digest,
                    generated_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
                };
                if let Some(dir) = cpath.parent() { let _ = fs::create_dir_all(dir); }
                if let Err(e) = fs::write(&cpath, serde_json::to_vec_pretty(&rec).unwrap()) {
                    out.ok=false; out.errors.push(format!("write chain: {}", e));
                }
            }
        }
    }

    out
}

If checks.rs already defines a DevCheckOutcome, either rename this one to DevCheckOutcomeEvents or reuse what you have. The code is otherwise self‑contained.

⸻

3) plugins/dev/src/lib.rs — wire the new checks

Make two small edits:

(a) Update --checks help to advertise new names)

// in augment_commands() -> dev doctor subcommand
.arg(
  Arg::new("checks")
    .long("checks")
    .value_name("CSV")
    .help("Subset of checks: graph,plugins,config,events,events-schema,events-chain,tools,cline (default: all)")
)

(b) Integrate in both CI and local doctor paths)

Inside the Some(("doctor", m)) => { ... } arm:
	•	CI branch (if m.get_flag("ci") { ... }) — extend the default set and push two new DoctorCiCheck entries:

let want: std::collections::HashSet<String> = m
    .get_one::<String>("checks")
    .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
    .unwrap_or_else(|| ["graph","plugins","config","events","events-schema","events-chain","tools","cline"]
        .into_iter().map(|s| s.to_string()).collect());

// ...

// events-schema
if want.contains("events-schema") {
    let outc = checks::check_events_schema(&cwd, None);
    checks.push(DoctorCiCheck {
        name: "events-schema".into(),
        ok: outc.ok,
        detail: format!("file={} lines={} errors={}", outc.file.unwrap_or_default(), outc.lines, outc.errors.len()),
        severity: if outc.ok { "info".into() } else { "error".into() },
        fix: Some("Inspect invalid lines in .nexoscli/log/events.jsonl; ensure all events decode to Event".into()),
    });
}

// events-chain (Verify in CI)
if want.contains("events-chain") {
    let outc = checks::check_events_chain(&cwd, checks::ChainMode::Verify);
    checks.push(DoctorCiCheck {
        name: "events-chain".into(),
        ok: outc.ok,
        detail: format!("file={} digest={:?} lines={} errs={}", outc.file.unwrap_or_default(), outc.digest, outc.lines, outc.errors.len()),
        severity: if outc.ok { "info".into() } else { "error".into() },
        fix: Some("Run locally: `nexoscli dev doctor --checks events-chain` to refresh chain; commit the updated chain".into()),
    });
}

	•	Local (non‑CI) branch — add both checks to the JSON report and honor --require:

Right after computing let since = m.get_one::<String>("since").unwrap(); add a parsed since_ts:

let since_ts: Option<OffsetDateTime> = match since {
    s if s.ends_with("h") => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
    s if s.ends_with("d") => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
    s => OffsetDateTime::parse(s, &Rfc3339).ok(),
};

Now compute and insert results (before report.insert("ready", ...)):

// 2b) Events schema (filtered by --since)
let ev_schema = crate::checks::check_events_schema(&std::env::current_dir()?, since_ts);
report.insert("events_schema".into(), serde_json::json!({
    "ok": ev_schema.ok, "lines": ev_schema.lines, "errors": ev_schema.errors, "file": ev_schema.file
}));
if (!ev_schema.ok) && require.iter().any(|c| *c == "events-schema") { hard_fail = true; }

// 2c) Events chain (local refresh)
let ev_chain = crate::checks::check_events_chain(&std::env::current_dir()?, crate::checks::ChainMode::Refresh);
report.insert("events_chain".into(), serde_json::json!({
    "ok": ev_chain.ok, "digest": ev_chain.digest, "lines": ev_chain.lines, "errors": ev_chain.errors, "file": ev_chain.file
}));
if (!ev_chain.ok) && require.iter().any(|c| *c == "events-chain") { hard_fail = true; }

Local runs will write .nexoscli/log/events.chain.json when missing / outdated.

⸻

4) New E2E test

nexoscli-bin/tests/dev_doctor_events_chain.rs

mod helpers; use helpers::*;
use std::fs;
use std::io::Write;

#[test]
fn doctor_events_schema_and_chain() {
    let t = tmp();

    // Produce some events
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    let _ = run_json(t.path(), &["task","new","--title","doctor chain smoke"]);
    let _ = run_json(t.path(), &["task","start","--task","TASK-19700101-doctor-chain-smoke"]);

    // Local: refresh chain file
    let v1 = run_json(t.path(), &["dev","doctor","--checks","events-schema,events-chain"]);
    assert!(v1["errors"].as_array().unwrap().is_empty());
    assert!(t.path().join(".nexoscli/log/events.chain.json").exists());

    // CI: verify passes
    let v2 = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-schema,events-chain","--require","events-schema,events-chain"]);
    assert_eq!(v2.status, 0);

    // Tamper with the events file → CI must fail
    let ev = r#"{"ts":"1970-01-01T00:00:00Z","plugin":"test","command":"tamper","exit_code":0}"#;
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(t.path().join(".nexoscli/log/events.jsonl"))
        .unwrap();
    writeln!(&mut f, "{}", ev).unwrap();

    let v3 = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-chain","--require","events-chain"]);
    assert_ne!(v3.status, 0, "CI verify must fail on chain drift");
}


⸻

5) Docs tweaks

docs/OBSERVABILITY.md — append:

### Chain‑of‑custody (events)

We compute a rolling digest over `.nexoscli/log/events.jsonl`:

H_0 = sha256(””)
H_n = sha256(H_{n-1} || “\n” || raw_line_n)

Local refresh:

```bash
nexoscli dev doctor --checks events-schema,events-chain

CI verify (no writes):

nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain

Manifest at .nexoscli/log/events.chain.json:

{
  "schema": "events_chain.v0.1",
  "file": ".nexoscli/log/events.jsonl",
  "from_ts": "2025-09-02T07:29:15Z",
  "to_ts": "2025-09-02T07:29:45Z",
  "lines": 42,
  "sha256": "sha256:…",
  "generated_at": "2025-09-02T07:30:00Z"
}

**`docs/PLAYBOOK.md`** — under “Health Checks” add:

```md
# Event integrity + chain-of-custody (recommended for CI)
nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain


⸻

🧪 How to run locally

cargo build --workspace

# create a few events
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli task new --title "doctor smoke"
./target/debug/nexoscli task start --task TASK-19700101-doctor-smoke

# local: writes/refreshes the chain
./target/debug/nexoscli dev doctor --checks events-schema,events-chain --output json | jq '.data'

# CI‑style verification (no writes)
./target/debug/nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain


⸻
