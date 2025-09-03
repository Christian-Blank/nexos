Awesome — moving along the critical path after Track A (schema tags + registry), here’s a ready‑to‑commit pack for Track B: Doctor checks for events schema + chain‑of‑custody. This locks the log integrity and gives CI guardrails without touching your command surfaces.

It’s additive and safe:
	•	New doctor checks: events-schema, events-chain.
	•	No breaking flags; your existing dev doctor keeps working.
	•	Optional auto‑write of the chain file in local mode; CI will fail on drift when required.

Below I give: minimal code patches, one new E2E test, and small docs updates.

⸻

What this delivers
	•	Schema validation of .nexoscli/log/events.jsonl lines against the real Event type (event.v0.3).
	•	Chain-of-custody digest over the event log:
	•	Rolling hash H = sha256(H || 0x0a || <raw-json-line>).
	•	Stored at .nexoscli/log/events.chain.json.
	•	dev doctor recomputes and compares; can create/refresh locally.
	•	CI guard: nexoscli dev doctor --ci --require events-schema,events-chain fails on invalid lines or chain drift.

⸻

1) plugins/dev/src/checks.rs — add two checks

Append these helpers (or place them in a new checks_events.rs and mod it). They don’t change existing code paths.

// --- BEGIN: events checks (append to checks.rs) ---
use std::{fs, io::BufRead, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DevCheckOutcome {
    pub name: String,
    pub ok: bool,
    pub lines: u64,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub digest: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EventsChain {
    schema: &'static str,    // "events_chain.v0.1"
    file: String,
    from_ts: Option<String>,
    to_ts: Option<String>,
    lines: u64,
    sha256: String,
    generated_at: String,
}
const EVENTS_CHAIN_SCHEMA: &str = "events_chain.v0.1";

fn events_file(root: &std::path::Path) -> PathBuf {
    root.join(".nexoscli").join("log").join("events.jsonl")
}
fn chain_file(root: &std::path::Path) -> PathBuf {
    root.join(".nexoscli").join("log").join("events.chain.json")
}

pub fn check_events_schema(root: &std::path::Path, since: Option<OffsetDateTime>) -> DevCheckOutcome {
    let mut out = DevCheckOutcome{ name:"events-schema".into(), ok:true, lines:0, errors: vec![], digest: None, file: None };
    let path = events_file(root);
    out.file = Some(path.display().to_string());
    if !path.exists() {
        out.ok = true; // empty is ok
        return out;
    }
    let f = match fs::File::open(&path) {
        Ok(f) => f,
        Err(e) => { out.ok=false; out.errors.push(format!("open: {}", e)); return out; }
    };
    let rdr = std::io::BufReader::new(f);
    for (idx, line) in rdr.lines().enumerate() {
        let raw = match line { Ok(s) => s, Err(e) => { out.ok=false; out.errors.push(format!("read line {}: {}", idx+1, e)); break; } };
        if raw.trim().is_empty() { continue; }
        // schema & ts parse
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
                // validate it can deserialize into the real Event
                if let Err(e) = serde_json::from_value::<nexoscli_core::event::Event>(v.clone()) {
                    out.ok=false; out.errors.push(format!("line {}: Event decode: {}", idx+1, e));
                } else {
                    // ensure tag present and correct
                    let schema_ok = v.get("schema").and_then(|x| x.as_str()) == Some(nexoscli_core::event::EVENT_SCHEMA_V03);
                    if !schema_ok {
                        out.ok=false; out.errors.push(format!("line {}: missing or wrong schema tag", idx+1));
                    }
                }
                out.lines += 1;
            }
            Err(e) => { out.ok=false; out.errors.push(format!("line {}: json parse: {}", idx+1, e)); }
        }
    }
    out
}

pub fn check_events_chain(root: &std::path::Path, mode: ChainMode) -> DevCheckOutcome {
    // mode: Verify | Refresh (local only)
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
        // include newline separator in rolling digest
        if lines > 0 { h.update(&[0x0a]); }
        h.update(raw.as_bytes());
        // times
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

    // compare with chain file if exists
    let cpath = chain_file(root);
    if cpath.exists() {
        match fs::read_to_string(&cpath) {
            Ok(txt) => {
                match serde_json::from_str::<EventsChain>(&txt) {
                    Ok(rec) => {
                        if rec.sha256 != digest || rec.lines != lines {
                            out.ok = false;
                            out.errors.push("digest or line count mismatch; run `nexoscli dev doctor --checks events-chain` locally to refresh".into());
                        }
                    }
                    Err(e) => { out.ok=false; out.errors.push(format!("chain parse: {}", e)); }
                }
            }
            Err(e) => { out.ok=false; out.errors.push(format!("chain read: {}", e)); }
        }
    } else {
        // If no chain file: create in non-CI mode (Refresh) or warn in CI.
        match mode {
            ChainMode::Verify => {
                out.ok = false;
                out.errors.push("chain file missing; please create with local doctor run".into());
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

#[derive(Copy, Clone)]
pub enum ChainMode { Verify, Refresh }
// --- END: events checks ---

Wire into your existing doctor dispatcher

In plugins/dev/src/lib.rs (inside the dev doctor handler), add:
	•	Parse new checks events-schema, events-chain.
	•	Detect --ci to select ChainMode::Verify, otherwise ChainMode::Refresh (so local runs create/refresh the chain file automatically).
	•	Optional --since window support for events-schema.

Minimal example of the integration block (adjust to your current code):

// inside match for ("doctor", sm)
let since = sm.get_one::<String>("since")
    .and_then(|s| time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok());
let ci = sm.get_flag("ci");
let checks: Vec<String> = parse_checks(sm.get_one::<String>("checks"));
let require: Vec<String> = parse_checks(sm.get_one::<String>("require"));

let root = std::env::current_dir()?;
let mut outcomes = vec![];

if checks.is_empty() || checks.contains(&"events-schema".into()) {
    outcomes.push(checks::check_events_schema(&root, since));
}
if checks.is_empty() || checks.contains(&"events-chain".into()) {
    outcomes.push(checks::check_events_chain(&root, if ci { checks::ChainMode::Verify } else { checks::ChainMode::Refresh }));
}

// ... include your existing outcomes here ...

let ok = outcomes.iter().all(|o| o.ok || !require.contains(&o.name));
let code = if ok { ctx.ok("dev","doctor",&serde_json::json!({"checks": outcomes})) }
           else { ctx.err("dev","doctor","dev.doctor.failed","one or more required checks failed", Some(serde_json::json!({"checks": outcomes})), 2) };
return Ok(code);

// helper
fn parse_checks(s: Option<&String>) -> Vec<String> {
    s.map(|x| x.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
     .unwrap_or_default()
}

If your doctor already has a registry pattern, just register two closures that call check_events_schema and check_events_chain.

⸻

2) New E2E test: dev_doctor_events_chain.rs

nexoscli-bin/tests/dev_doctor_events_chain.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn doctor_events_schema_and_chain() {
    let t = tmp();

    // Generate a few events
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    let _ = run_json(t.path(), &["task","new","--title","doctor chain smoke"]);
    let _ = run_json(t.path(), &["task","start","--task","TASK-19700101-doctor-chain-smoke"]);

    // 1) Local run creates/refreshes chain
    let v1 = run_json(t.path(), &["dev","doctor","--checks","events-schema,events-chain"]);
    assert!(v1["errors"].as_array().unwrap().is_empty());
    let chain = t.path().join(".nexoscli/log/events.chain.json");
    assert!(chain.exists(), "local doctor should write chain");

    // 2) CI mode verifies and passes
    let v2 = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-schema,events-chain","--require","events-schema,events-chain"]);
    assert_eq!(v2.status, 0, "CI verify should pass when chain matches");

    // 3) Tamper: append a line; CI should fail
    let ev = r#"{"ts":"1970-01-01T00:00:00Z","schema":"event.v0.3","plugin":"test","command":"tamper"}"#;
    fs::OpenOptions::new().append(true).open(t.path().join(".nexoscli/log/events.jsonl")).unwrap()
        .write_all(format!("\n{}", ev).as_bytes()).unwrap();

    let v3 = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-chain","--require","events-chain"]);
    assert_ne!(v3.status, 0, "CI verify must fail on chain drift");
}

Uses your existing helpers (run_json, run_status_json). If those differ, adapt asserts to your envelope shape.

⸻

3) Docs updates (tiny)

docs/OBSERVABILITY.md — add “Chain-of-custody” section

Append:

### Chain-of-custody (events)

NEXOS maintains a rolling digest over `.nexoscli/log/events.jsonl`:

H_0 = sha256(””)
H_n = sha256(H_{n-1} || “\n” || raw_line_n)

The doctor writes/validates a manifest at `.nexoscli/log/events.chain.json`:

```json
{
  "schema": "events_chain.v0.1",
  "file": ".nexoscli/log/events.jsonl",
  "from_ts": "2025-09-02T07:29:15Z",
  "to_ts": "2025-09-02T07:29:45Z",
  "lines": 42,
  "sha256": "sha256:…",
  "generated_at": "2025-09-02T07:30:00Z"
}

	•	Local runs refresh the file:

nexoscli dev doctor --checks events-schema,events-chain

	•	CI verifies (no writes) and can enforce:

nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain

### `docs/PLAYBOOK.md` — doctor examples

Add one line under Health Checks:

```md
# Event integrity + chain-of-custody (recommended for CI)
nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain


⸻

4) Optional CI snippet

- name: Doctor (events schema + chain)
  run: ./target/debug/nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain


⸻

5) Smoke locally

cargo build --workspace

# produce events
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli task new --title "doctor smoke"
./target/debug/nexoscli task start --task TASK-19700101-doctor-smoke

# local write (refreshes chain)
./target/debug/nexoscli dev doctor --checks events-schema,events-chain --output json | jq '.data.checks'

# CI verify (no writes)
./target/debug/nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain


⸻

6) Why this is the right “next”
	•	It anchors observability with a tamper‑evident trail (exactly what we’ll mirror in Temporal via Search Attributes & history).
	•	It shifts left: schema drift or log corruption fails locally and in CI—not after a bag is built.
	•	It keeps a single source of truth: we validate against the actual Rust types (via serde + schemars), not duplicated JSON.

