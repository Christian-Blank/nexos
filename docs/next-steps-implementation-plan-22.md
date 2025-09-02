Perfect—let’s ship T‑1: the Temporal outbox adapter.
This adds --engine temporal to workflow run (and workflow signal) so instead of executing locally, the runner emits envelopes to .nexoscli/engine/outbox/:
	•	workflow.start — run metadata (build_id, change_ids, memo, search_attributes, spec_sha)
	•	one activity.schedule / timer.schedule / child.start per step
	•	workflow.signal — when signaling with --engine temporal

No SDK, no network—just outbox JSON the future worker can consume. Local (--engine local) behavior is unchanged.

Below are drop‑in patches for plugins/workflow. They compose with your current file (timers, locks, child workflows, visibility, determinism/Change‑IDs from T‑2).

⸻

1) Cargo changes — plugins/workflow/Cargo.toml

Add uuid (for run IDs) and regex if you didn’t already add it in T‑2:

[dependencies]
# ...existing...
uuid = { version = "1", features = ["v4"] }
regex = { version = "1" }


⸻

2) CLI: add --engine to run and signal

In plugins/workflow/src/lib.rs:

2.1 Engine kind

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineKind { Local, Temporal }

impl std::str::FromStr for EngineKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(EngineKind::Local),
            "temporal" => Ok(EngineKind::Temporal),
            _ => Err("must be one of: local, temporal".into()),
        }
    }
}

2.2 Extend commands

In augment_commands where you define run and signal:

.subcommand(
  ClapCommand::new("run")
    .about("Run a workflow spec sequentially")
    .arg(Arg::new("spec").long("spec").required(true).value_name("YAML"))
    .arg(Arg::new("engine").long("engine").value_parser(["local","temporal"]).default_value("local"))
    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
    .arg(Arg::new("build-id").long("build-id").value_name("STRING"))
    .arg(Arg::new("allow-change-ids-drift").long("allow-change-ids-drift").action(ArgAction::SetTrue))
)
.subcommand(
  ClapCommand::new("signal")
    .about("Send a signal to a workflow (file mailbox or temporal outbox)")
    .arg(Arg::new("id").long("id").required(true))
    .arg(Arg::new("name").long("name").required(true))
    .arg(Arg::new("payload").long("payload").value_name("JSON").default_value("{}"))
    .arg(Arg::new("engine").long("engine").value_parser(["local","temporal"]).default_value("local"))
)


⸻

3) Outbox envelopes (schema + helpers)

Add near your other helpers:

use uuid::Uuid;
use std::{fs, path::{Path, PathBuf}, io::Write as _};

#[derive(Debug, Serialize)]
struct OutboxEnvelope {
    op: String,                   // e.g., workflow.start, activity.schedule, timer.schedule, child.start, workflow.signal
    workflow: String,             // spec.id
    run_id: String,               // WF-RUN-<uuid>
    key: String,                  // spec.id (partition / routing key)
    name: Option<String>,         // activity name, signal name, or child id
    attrs: OutboxAttrs,
    body: serde_json::Value,
    ts: String,
}

#[derive(Debug, Serialize)]
struct OutboxAttrs {
    build_id: Option<String>,
    change_ids: Vec<String>,
    spec_sha256: String,
    search_attributes: std::collections::BTreeMap<String, String>,
    memo: std::collections::BTreeMap<String, serde_json::Value>,
}

fn outbox_dir() -> PathBuf {
    PathBuf::from(".nexoscli").join("engine").join("outbox")
}

fn write_outbox(env: &OutboxEnvelope) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(outbox_dir())?;
    let fname = format!(
        "{}_{}_{}_{}.json",
        env.ts.replace(':','-'),
        env.op.replace('.','-'),
        env.workflow,
        &env.run_id[0..8]
    );
    let path = outbox_dir().join(fname);
    let mut f = fs::OpenOptions::new().create_new(true).write(true).open(&path)?;
    f.write_all(serde_json::to_vec_pretty(env)?.as_slice())?;
    Ok(path)
}

fn new_run_id() -> String {
    format!("WF-RUN-{}", Uuid::new_v4())
}


⸻

4) Temporal mode for workflow run

In your execute() match arm for "run":

Some(("run", sm)) => {
    let spec_path = sm.get_one::<String>("spec").unwrap();
    let engine: EngineKind = sm.get_one::<String>("engine").unwrap().parse().unwrap();
    let allow_drift = sm.get_flag("allow-change-ids-drift");
    let build_id_cli = sm.get_one::<String>("build-id").cloned();

    // Load, optional override, and compute SHA
    let spec_txt = fs::read_to_string(spec_path)?;
    let mut spec = serde_yaml::from_str::<WorkflowSpec>(&spec_txt)?;
    if let Some(b) = build_id_cli { spec.run.build_id = Some(b); }
    let spec_sha = sha256(&spec_txt.as_bytes());

    // Validate
    let vrep = validate_spec(&spec)?;
    if !vrep.errors.is_empty() {
        return Ok(ctx.err("workflow","validate","workflow.validate",
            "spec failed validation", Some(serde_json::to_value(vrep)?), 64));
    }

    // Determinism gate (from T‑2)
    // (Assumes you already have read_determinism/write_determinism/determinism_path/now/append_history)
    let hist = history_path(&spec.id)?;
    ensure_parent_dir(hist.to_str().unwrap())?;
    let det_prev = read_determinism(&spec.id);
    let now_ts = now();
    let change_ids = spec.run.change_ids.clone();

    if let Some(prev) = &det_prev {
        let mut a = prev.change_ids.clone(); a.sort();
        let mut b = change_ids.clone();      b.sort();
        if a != b && !allow_drift && engine == EngineKind::Local {
            append_history(&hist, &json!({"ts": now(), "event":"change_ids_drift","prev": a,"now": b}))?;
            return Ok(ctx.err("workflow","run.change_ids_drift",
                "workflow.change_ids_drift",
                "run.change_ids changed; pass --allow-change-ids-drift to proceed",
                Some(json!({"prev": a, "now": b})), 66));
        }
    }

    // Temporal engine → emit envelopes, don't execute
    if engine == EngineKind::Temporal {
        let run_id = new_run_id();

        // 1) workflow.start
        let start_env = OutboxEnvelope{
            op: "workflow.start".into(),
            workflow: spec.id.clone(),
            run_id: run_id.clone(),
            key: spec.id.clone(),
            name: Some("start".into()),
            attrs: OutboxAttrs{
                build_id: spec.run.build_id.clone(),
                change_ids: spec.run.change_ids.clone(),
                spec_sha256: spec_sha.clone(),
                search_attributes: spec.run.search_attributes.clone(),
                memo: spec.run.memo.clone(),
            },
            body: json!({
                "spec_path": spec_path,
                "step_count": spec.steps.len(),
            }),
            ts: now(),
        };
        let _ = write_outbox(&start_env)?;

        // 2) step schedules
        let mut scheduled = 0usize;
        for s in &spec.steps {
            let (op, name, body) = if let Some(t) = &s.timer {
                ("timer.schedule".to_string(),
                 Some(s.id.clone()),
                 json!({"workflow_id": spec.id, "step_id": s.id, "duration_ms": t.duration_ms}))
            } else if let Some(c) = &s.child_workflow {
                ("child.start".to_string(),
                 Some(s.id.clone()),
                 json!({"workflow_id": spec.id, "step_id": s.id, "child_spec": c.spec, "wait": c.wait, "inherit_search_attributes": c.inherit_search_attributes}))
            } else {
                // activity
                let act = s.activity.clone().unwrap_or_else(|| "unknown".into());
                ("activity.schedule".to_string(),
                 Some(act.clone()),
                 json!({"workflow_id": spec.id, "step_id": s.id, "activity": act, "args": s.args, "idempotent": s.idempotent, "retry": s.retry, "timeouts": s.timeouts, "resources": s.resources }))
            };

            let env = OutboxEnvelope{
                op, workflow: spec.id.clone(), run_id: run_id.clone(), key: spec.id.clone(), name,
                attrs: OutboxAttrs{
                    build_id: spec.run.build_id.clone(),
                    change_ids: spec.run.change_ids.clone(),
                    spec_sha256: spec_sha.clone(),
                    search_attributes: spec.run.search_attributes.clone(),
                    memo: spec.run.memo.clone(),
                },
                body, ts: now(),
            };
            let _ = write_outbox(&env)?;
            scheduled += 1;
        }

        // 3) determinism record (engine=temporal)
        let det = Determinism {
            build_id: spec.run.build_id.clone(),
            change_ids: spec.run.change_ids.clone(),
            spec_sha256: spec_sha.clone(),
            first_run_at: det_prev.as_ref().map(|d| d.first_run_at.clone()).unwrap_or_else(|| now_ts.clone()),
            last_run_at: now_ts,
            engine: "temporal".into(),
        };
        write_determinism(&spec.id, &det)?;

        return Ok(ctx.ok("workflow","run.temporal_outbox",
            &json!({"id": spec.id, "run_id": run_id, "scheduled": scheduled})));
    }

    // Local engine (existing behavior) continues below…
    // (your current planning/idempotency/locks/timers/child execution path)
}

Note: We gate change_ids drift only for local execution; outbox production is often part of migration where you may want to emit despite drift. If you prefer stricter, move the gate above the engine switch.

⸻

5) Temporal mode for workflow signal

In execute() arm for "signal":

Some(("signal", sm)) => {
    let id = sm.get_one::<String>("id").unwrap().to_string();
    let name = sm.get_one::<String>("name").unwrap().to_string();
    let payload_s = sm.get_one::<String>("payload").unwrap().to_string();
    let engine: EngineKind = sm.get_one::<String>("engine").unwrap().parse().unwrap();

    if engine == EngineKind::Temporal {
        // Minimal outbox signal (no spec read needed)
        let env = OutboxEnvelope{
            op: "workflow.signal".into(),
            workflow: id.clone(),
            run_id: new_run_id(),
            key: id.clone(),
            name: Some(name.clone()),
            attrs: OutboxAttrs{
                build_id: None, change_ids: vec![], spec_sha256: "".into(),
                search_attributes: std::collections::BTreeMap::new(),
                memo: std::collections::BTreeMap::new(),
            },
            body: serde_json::from_str::<serde_json::Value>(&payload_s).unwrap_or_else(|_| json!({})),
            ts: now(),
        };
        let path = write_outbox(&env)?;
        return Ok(ctx.ok("workflow","signal.temporal_outbox",
            &json!({"id": id, "name": name, "outbox": path})));
    }

    // Local engine: your existing file‑mailbox signal behavior
    // (… current implementation …)
}


⸻

6) Utility: sha256 helper over bytes (used above)

If you currently only have sha256(&[u8]) on slices, keep it. If not, add this trivial adapter:

fn sha256(bytes: &[u8]) -> String { format!("sha256:{:x}", Sha256::digest(bytes)) }


⸻

7) Minimal docs update (append)

docs/WORKFLOWS_PLUGIN.md — add:

### Temporal outbox (T‑1)

Run with `--engine temporal` to emit envelopes under `.nexoscli/engine/outbox/` instead of executing:

```bash
nexoscli workflow run --spec docs/workflows/WF-demo.yaml --engine temporal --output json
# emits:
#  - workflow.start
#  - activity.schedule / timer.schedule / child.start  (per step)

Signals can also target the outbox:

nexoscli workflow signal --id WF-demo --name pause --engine temporal

Envelope shape (example):

{
  "op": "activity.schedule",
  "workflow": "WF-demo",
  "run_id": "WF-RUN-6b5e…",
  "key": "WF-demo",
  "name": "issue.new",
  "attrs": {
    "build_id": "build-A",
    "change_ids": ["CH_STEP_ORDER_V1"],
    "spec_sha256": "sha256:…",
    "search_attributes": { "task_queue": "nexos", "owner": "christian" },
    "memo": {}
  },
  "body": { "workflow_id": "WF-demo", "step_id": "s1", "activity": "issue.new", "args": { "title": "hi" } },
  "ts": "2025-09-02T12:34:56Z"
}

---

## 8) E2E tests — `nexoscli-bin/tests/workflow_temporal_outbox.rs`

```rust
use assert_cmd::prelude::*;
use predicates::str::contains;
use std::{process::Command, fs};

#[test]
fn emits_outbox_for_activity_and_timer() {
    let spec = r#"version: "0.1"
id: WF-outbox
run: { build_id: "build-A", change_ids: ["CH_V1"], search_attributes: { owner: "christian" } }
steps:
  - id: a1
    activity: "issue.new"
    args: { title: "test" }
    idempotent: true
  - id: t1
    timer: { duration_ms: 50 }
"#;
    std::fs::create_dir_all(".nexoscli/workflows/WF-outbox").unwrap();
    std::fs::write(".nexoscli/workflows/WF-outbox/spec.yaml", spec).unwrap();

    Command::cargo_bin("nexoscli").unwrap()
      .args(["workflow","run","--spec",".nexoscli/workflows/WF-outbox/spec.yaml","--engine","temporal","--output","json"])
      .assert().success().stdout(contains("\"scheduled\":2"));

    // Outbox contains start + 2 schedules
    let entries = fs::read_dir(".nexoscli/engine/outbox").unwrap()
        .filter_map(|e| e.ok()).map(|e| e.file_name().into_string().unwrap())
        .filter(|n| n.contains("WF-outbox")).collect::<Vec<_>>();
    assert!(entries.iter().any(|n| n.contains("workflow-start")));
    assert!(entries.iter().any(|n| n.contains("activity-schedule")));
    assert!(entries.iter().any(|n| n.contains("timer-schedule")));
}

#[test]
fn signal_writes_outbox() {
    // engine temporal signal should write envelope without spec
    Command::cargo_bin("nexoscli").unwrap()
      .args(["workflow","signal","--id","WF-outbox","--name","pause","--engine","temporal","--output","json"])
      .assert().success();
}


⸻

9) Acceptance criteria (for T‑1)
	•	workflow run --engine temporal does not execute steps; it writes:
	•	one workflow.start envelope and one schedule envelope per step to .nexoscli/engine/outbox/.
	•	Envelopes include: build_id, change_ids, spec_sha, memo, search_attributes.
	•	workflow signal --engine temporal writes a workflow.signal envelope.
	•	Local execution (--engine local, default) remains unchanged (idempotency, locks, timers, child workflows).
	•	Determinism record (determinism.json) gets updated with engine: "temporal" when emitting outbox.
	•	Tests pass and outbox files are created with the expected contents.

⸻

Why this sets you up to “flip the switch”
	•	Your specs stay identical. Only --engine temporal changes behavior.
	•	The outbox schema is already the worker contract; a tiny Rust or Python worker can consume these files and call Temporal SDK methods (StartWorkflow, ScheduleActivity, StartChildWorkflow, StartTimer, SignalWorkflow) 1:1.
	•	Because you already have effects/idempotency at the CLI boundary, replaying a workflow won’t double‑apply side‑effects when your worker re‑drives activities.
