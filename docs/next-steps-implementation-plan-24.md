You’re exactly right to make observability + replay the center of gravity. If we get every action fully recorded, verifiable, and replayable, everything else (Temporal, agents, safety, SLOs) becomes a plug‑in.

Below is a tight hardening plan for a bullet‑proof recording & replay foundation—plus ready‑to‑commit files and stubs to wire in now. It builds on what you already have (events.jsonl, TracePacks, Cline ingest) and adds: hash‑chained events, run manifests, replay verification, and metrics/SLO surfaces.

⸻

Critical path (observability & replay, v1)

Goal: deterministic, tamper‑evident event ledger; portable TracePacks; a verifier; and first SLO metrics—before we touch Temporal.

A. Event Ledger v0.3 (hash‑chained)
	•	What: Each event line is extended with schema, seq, prev, content_hash, chain_hash.
	•	Why: Makes the log tamper‑evident; enables fast verification and consistent replay planning.
	•	How: Compute content_hash = sha256(canonical_event) (no chain fields). Then chain_hash = sha256(prev + content_hash).
	•	Where written: still .nexoscli/log/events.jsonl (append‑only). Also write per‑run file .nexoscli/log/runs/<RUN-ID>.jsonl and a lightweight .nexoscli/log/runs/<RUN-ID>.manifest.json.

B. TracePack v0.2 (anchored)
	•	What: Add chain anchors and per‑run manifest to the bundle; keep your current layout.
	•	Why: “Lift and verify anywhere” with no dependency on the host repo.
	•	How: Include run_manifest.json with {first_seq,last_seq,first_hash,last_hash,count} and a chain_ok flag set by the verifier.

C. Replay Verify (CLI)
	•	What: nexoscli replay verify verifies chain integrity, schemas, and (optionally) patch applicability.
	•	Why: Gives you a binary “ledger OK” signal in CI and locally. It’s your tamper detector.

D. Metrics & SLOs (first slice)
	•	What: nexoscli dev telemetry report gains a --metrics flag that emits stable JSON counters + SLO judgments (pass/fail).
	•	Why: Data → decisions. Start with 5 metrics you can compute from events alone; add more later.

⸻

Schemas & formats (drop‑in)

1) Event schema v0.3 (commit at docs/SPECS/event_v0.3.schema.json)

{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "nexoscli.event.v0.3",
  "type": "object",
  "required": ["schema","ts","run_id","seq","plugin","command","exit_code","dur_ms","content_hash","chain_hash"],
  "properties": {
    "schema": { "const": "event.v0.3" },
    "ts": { "type":"string","format":"date-time" },
    "run_id": { "type":"string" },
    "seq": { "type":"integer","minimum": 1 },
    "prev": { "type":"string" },
    "content_hash": { "type":"string","pattern":"^sha256:[0-9a-f]{64}$" },
    "chain_hash": { "type":"string","pattern":"^sha256:[0-9a-f]{64}$" },

    "plugin": { "type":"string" },
    "command": { "type":"string" },
    "args_hash": { "type":"string" },
    "exit_code": { "type":"integer" },
    "dur_ms": { "type":"integer","minimum":0 },
    "attrs": { "type":"object","additionalProperties": true },
    "meta": { "type":"object","additionalProperties": true }
  },
  "additionalProperties": true
}

2) Run manifest (include in TracePack; also persisted locally)

docs/SPECS/run_manifest_v0.2.schema.json (abbrev):

{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "nexoscli.run_manifest.v0.2",
  "type": "object",
  "required": ["version","run_id","count","first_seq","last_seq","first_hash","last_hash","time_range"],
  "properties": {
    "version": { "const":"0.2" },
    "run_id": { "type":"string" },
    "count": { "type":"integer","minimum":1 },
    "first_seq": { "type":"integer","minimum":1 },
    "last_seq": { "type":"integer","minimum":1 },
    "first_hash": { "type":"string" },
    "last_hash": { "type":"string" },
    "time_range": {
      "type":"object",
      "required":["from","to"],
      "properties": { "from":{"type":"string","format":"date-time"}, "to":{"type":"string","format":"date-time"} }
    },
    "chain_ok": { "type":"boolean" }
  }
}

Note: These schemas let you validate both at record time (soft) and in CI (hard).

⸻

Implementation (additive, minimal churn)

You already emit envelopes via ctx.ok/ctx.err. We’ll add a small Ledger helper in nexoscli-core and call it from the existing event writer so the change is centralized.

A) Core: ledger helper (new file)

nexoscli-core/src/ledger.rs

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Serialize, Clone)]
pub struct EventV03<'a> {
    pub schema: &'static str,          // "event.v0.3"
    pub ts: String,                    // RFC3339
    pub run_id: &'a str,
    pub seq: u64,
    pub prev: String,                  // "" for seq=1
    pub content_hash: String,
    pub chain_hash: String,

    pub plugin: &'a str,
    pub command: &'a str,
    pub args_hash: Option<String>,
    pub exit_code: i32,
    pub dur_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

pub struct Ledger {
    run_id: String,
    seq: u64,
    last_chain_hash: String,
    events_path: PathBuf,
    run_path: PathBuf,
    // we keep first/last bookkeeping to write a run manifest on drop
    first_seq: u64,
    first_hash: Option<String>,
    last_seq: u64,
    first_ts: Option<String>,
    last_ts: Option<String>,
}

impl Ledger {
    pub fn open(base: &str, run_id: &str) -> anyhow::Result<Self> {
        let root = PathBuf::from(base);
        fs::create_dir_all(root.join("log").join("runs"))?;
        Ok(Self {
            run_id: run_id.to_string(),
            seq: 0,
            last_chain_hash: String::new(),
            events_path: root.join("log").join("events.jsonl"),
            run_path: root.join("log").join("runs").join(format!("{}.jsonl", run_id)),
            first_seq: 0,
            first_hash: None,
            last_seq: 0,
            first_ts: None,
            last_ts: None,
        })
    }

    /// Append one event; `env_core` is the original envelope fields used to compute content hash.
    pub fn append(
        &mut self,
        plugin: &str,
        command: &str,
        exit_code: i32,
        dur_ms: u64,
        args_hash: Option<String>,
        attrs: Option<serde_json::Value>,
        meta: Option<serde_json::Value>,
        canonical_for_hash: &serde_json::Value,
    ) -> anyhow::Result<()> {
        self.seq += 1;
        let ts = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();

        let content_hash = sha256(&serde_json::to_vec(canonical_for_hash)?);
        let chain_hash = sha256(format!("{}{}", self.last_chain_hash, &content_hash).as_bytes());

        let ev = EventV03 {
            schema: "event.v0.3",
            ts: ts.clone(),
            run_id: &self.run_id,
            seq: self.seq,
            prev: self.last_chain_hash.clone(),
            content_hash,
            chain_hash: chain_hash.clone(),
            plugin,
            command,
            args_hash,
            exit_code,
            dur_ms,
            attrs,
            meta,
        };

        // update running fields
        if self.first_seq == 0 { self.first_seq = self.seq; self.first_hash = Some(ev.chain_hash.clone()); self.first_ts = Some(ts.clone()); }
        self.last_seq = self.seq;
        self.last_chain_hash = chain_hash;
        self.last_ts = Some(ts.clone());

        // append to per-run and global (append-only)
        let line = serde_json::to_string(&ev)? + "\n";
        append(&self.run_path, &line)?;
        append(&self.events_path, &line)?;
        Ok(())
    }
}

impl Drop for Ledger {
    fn drop(&mut self) {
        // write run manifest (best-effort)
        if self.last_seq >= self.first_seq {
            let manifest = json!({
              "version":"0.2",
              "run_id": self.run_id,
              "count": (self.last_seq - self.first_seq + 1),
              "first_seq": self.first_seq,
              "last_seq": self.last_seq,
              "first_hash": self.first_hash,
              "last_hash": self.last_chain_hash,
              "time_range": { "from": self.first_ts, "to": self.last_ts },
              "chain_ok": true
            });
            if let Some(dir) = self.run_path.parent() {
                let mf = dir.join(format!("{}.manifest.json", self.run_id));
                let _ = fs::write(mf, serde_json::to_vec_pretty(&manifest).unwrap_or_default());
            }
        }
    }
}

fn append(path: &PathBuf, s: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() { fs::create_dir_all(dir)?; }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(s.as_bytes())?;
    Ok(())
}

fn sha256(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    format!("sha256:{:x}", h.finalize())
}

Hook point: Call Ledger::open(".nexoscli", run_id) once per process (you already generate run_id). Then in the single place where ctx.ok/ctx.err writes an event, also call ledger.append(...), passing the canonical envelope (the minimal, stable subset: plugin, command, exit_code, dur_ms, args_hash, attrs, meta). This keeps hashing stable across cosmetic changes.

B) Wire ledger into the existing event path
	•	In nexoscli-core/src/envelope.rs (or wherever you emit events), instantiate a singleton Ledger in the CommandContext (or engine), and call append(...) after each result.
	•	Use attrs for command‑specific details; keep it lean to avoid huge lines.

If you’d like, you can guard this with NEXOS_LEDGER=0/1 while you roll it out.

⸻

Replay hardening (plugin additions)

1) Verify command

plugins/replay/src/lib.rs — add a new subcommand:

// inside augment_commands
.subcommand(
  ClapCommand::new("verify").about("Verify event chain integrity for a run or a file")
    .arg(Arg::new("run-id").long("run-id").value_name("RUN").required_unless_present("file"))
    .arg(Arg::new("file").long("file").value_name("PATH").required_unless_present("run-id"))
    .arg(Arg::new("strict").long("strict").action(ArgAction::SetTrue))
)
// in execute()
Some(("verify", sm)) => {
  let run_id = sm.get_one::<String>("run-id").cloned();
  let file = sm.get_one::<String>("file").map(|s| s.into());
  let strict = sm.get_flag("strict");
  let res = verify_chain(run_id, file, strict)?;
  let exit = if res.ok {
    ctx.ok("replay","verify",&serde_json::to_value(&res)?)
  } else {
    ctx.err("replay","verify","replay.verify.failed","chain verification failed", Some(serde_json::to_value(&res)?), 2)
  };
  Ok(exit)
}

Add a small verifier (pseudocode-ish, easy to wire to your existing JSON/event reader):

#[derive(Serialize)]
struct VerifyResult { ok: bool, count: usize, first_seq: u64, last_seq: u64, first_hash: String, last_hash: String, diffs: Vec<String> }

fn verify_chain(run_id: Option<String>, file: Option<std::path::PathBuf>, strict: bool) -> anyhow::Result<VerifyResult> {
    let mut diffs = vec![];
    let mut last_chain = String::new();
    let mut last_seq = 0u64;
    let mut first_seq = 0u64;
    let mut first_hash = String::new();
    let mut count = 0usize;

    let lines: Box<dyn Iterator<Item=String>> = if let Some(p) = file {
        Box::new(std::fs::read_to_string(p)?.lines().map(|s| s.to_string()).collect::<Vec<_>>().into_iter())
    } else {
        let p = PathBuf::from(".nexoscli").join("log").join("runs").join(format!("{}.jsonl", run_id.unwrap()));
        Box::new(std::fs::read_to_string(p)?.lines().map(|s| s.to_string()).collect::<Vec<_>>().into_iter())
    };

    for line in lines {
        if line.trim().is_empty() { continue; }
        let v: serde_json::Value = serde_json::from_str(&line)?;
        // required fields
        let ev_schema = v.get("schema").and_then(|x| x.as_str()).unwrap_or("");
        if strict && ev_schema != "event.v0.3" { diffs.push(format!("schema mismatch: {}", ev_schema)); }
        let seq = v.get("seq").and_then(|x| x.as_u64()).unwrap_or(0);
        let prev = v.get("prev").and_then(|x| x.as_str()).unwrap_or("");
        let content_hash = v.get("content_hash").and_then(|x| x.as_str()).unwrap_or("");
        let chain_hash = v.get("chain_hash").and_then(|x| x.as_str()).unwrap_or("");

        // recompute chain hash (no recompute of content hash here; optional strictness)
        let recomputed = sha256(format!("{}{}", last_chain, content_hash).as_bytes());
        if seq == 0 || (last_seq != 0 && seq != last_seq + 1) {
            diffs.push(format!("seq gap at {}", seq));
        }
        if seq == 1 {
            first_seq = 1;
            first_hash = chain_hash.to_string();
        } else {
            if prev != last_chain {
                diffs.push(format!("prev mismatch at seq {}", seq));
            }
            if chain_hash != recomputed {
                diffs.push(format!("chain_hash mismatch at seq {}", seq));
            }
        }
        last_chain = chain_hash.to_string();
        last_seq = seq;
        count += 1;
    }

    Ok(VerifyResult {
        ok: diffs.is_empty() && count>0,
        count, first_seq, last_seq, first_hash, last_hash: last_chain, diffs
    })
}

Optional: --strict can also recompute content_hash from a canonicalized subset of the event if you want the deepest guarantee.

2) Update TracePack extractor to include run manifest

You already have replay extract. Add:
	•	read .nexoscli/log/runs/<RUN-ID>.manifest.json if present; else compute from lines and write run_manifest.json into the pack.
	•	run verify_chain and set manifest.chain_ok.

⸻

Metrics & SLOs (first slice)

Augment nexoscli dev telemetry report with a stable machine output block:

{
  "metrics": {
    "events_total": 1542,
    "runs_total": 28,
    "task_coverage": 0.63,
    "fail_rate": 0.07,
    "median_cmd_dur_ms": 420
  },
  "slo": {
    "task_coverage_min": { "target": 0.6, "actual": 0.63, "pass": true },
    "fail_rate_max":     { "target": 0.1, "actual": 0.07, "pass": true }
  }
}

Compute now (no new deps):
	•	events_total: count of lines in window.
	•	runs_total: distinct run_id count.
	•	task_coverage: fraction of events with meta.task_id.
	•	fail_rate: exit_code!=0 / total.
	•	median_cmd_dur_ms: median dur_ms.

CI guardrail: extend dev doctor --ci to accept --require events_integrity which calls replay verify for the N most recent runs and fails if any chain breaks.

⸻

Documentation updates (small, precise)
	•	docs/OBSERVABILITY.md: add a short “Event v0.3” section with the new fields and a 3‑line example.
	•	docs/TRACEPACK.md: bump to v0.2, add run_manifest.json and chain_ok flag.
	•	docs/TEMPORAL_PARITY.md: note “ledger present locally; Temporal mapping = History + Memos; verification runs pre‑submit”.

⸻

Tests to add (E2E)
	1.	nexoscli-bin/tests/envelope_hash_chain.rs
	•	Run two commands via term run (or any) inside an active task.
	•	Assert .nexoscli/log/runs/<RUN-ID>.jsonl exists; read two lines; check seq increments and chain_hash equals sha256(prev + content_hash).
	2.	nexoscli-bin/tests/replay_verify.rs
	•	replay verify --run-id <id> returns { ok: true }.
	•	Corrupt one line; verify must fail with a helpful diff.
	3.	nexoscli-bin/tests/telemetry_metrics.rs
	•	Create a mix of success/failure events; assert task_coverage and fail_rate numbers in dev telemetry report --metrics --since 1h.

⸻

CI additions (tiny)
	•	Job: Events integrity

nexoscli dev telemetry report --since 24h --metrics --output json
nexoscli replay verify --run-id $(jq -r '.data.runs[0].run_id' <report.json>)


	•	Doctor gate:

nexoscli dev doctor --ci --require events,plugins,graph,events_integrity



⸻

Why this nails your priorities
	•	Deterministic & replayable: hash‑chained events + per‑run manifest + portable TracePacks = verifiable history anywhere.
	•	Data‑first: metrics & SLOs come straight from the ledger; they’re not ad‑hoc.
	•	Agent‑friendly: stable schemas + JSON outputs make it trivial for AI workflows to reason/act.
	•	Temporal‑ready: chain and manifests are orthogonal to the engine. When you flip to Temporal, you just stream the same envelopes; verification & SLOs stay identical.

⸻

Micro‑backlog (open as issues) add to ROADMAP_NEXT.md as suggestion
	1.	core/ledger: add v0.3 writer and call from CommandContext.
	2.	replay/verify: implement chain verification + run manifest generation.
	3.	tracepack/v0.2: include run_manifest.json; set chain_ok.
	4.	dev/telemetry: --metrics slice (5 metrics) + JSON contract.
	5.	doctor/events_integrity: verify last N runs; fail on chain break.
	6.	docs: update OBSERVABILITY/TRACEPACK; add schemas under docs/SPECS/.
	7.	tests: three E2Es above.

⸻

If you want, I can also draft the minimal CommandContext patch to initialize Ledger once per process and call append(...) where ctx.ok/ctx.err emits results. That patch is <30 LOC if your envelope code is centralized.