What you get now
	•	A core trait WorkflowEngine + Engine::Local and Engine::TemporalStub.
	•	CLI flag --engine local|temporal on task start|stage|end.
	•	With --engine temporal, we don’t run hooks; instead we write a ready‑to‑send envelope to .nexoscli/engine/outbox/ (this is your safe “dry‑client”).
	•	With --engine local, behavior is unchanged (hooks/gates run as they do today).

1) Core: engine trait + outbox stub

nexoscli-core/src/engine.rs (new)

use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub trait WorkflowEngine {
    fn start(&self, wf: &str, key: &str, attrs: Value, input: Value) -> anyhow::Result<()>;
    fn signal(&self, wf: &str, key: &str, name: &str, attrs: Value, payload: Value) -> anyhow::Result<()>;
    fn query (&self, wf: &str, key: &str, name: &str, args: Value) -> anyhow::Result<Value>;
}

#[derive(Clone, Copy, Debug)]
pub enum EngineKind { Local, TemporalStub }

pub struct LocalEngine;
impl WorkflowEngine for LocalEngine {
    fn start(&self, _wf: &str, _key: &str, _attrs: Value, _input: Value) -> anyhow::Result<()> { Ok(()) }
    fn signal(&self, _wf: &str, _key: &str, _name: &str, _attrs: Value, _payload: Value) -> anyhow::Result<()> { Ok(()) }
    fn query (&self, _wf: &str, _key: &str, _name: &str, _args: Value) -> anyhow::Result<Value> { Ok(Value::Null) }
}

pub struct TemporalOutboxEngine;
impl WorkflowEngine for TemporalOutboxEngine {
    fn start(&self, wf: &str, key: &str, attrs: Value, input: Value) -> anyhow::Result<()> {
        write_outbox("start", wf, key, None, attrs, input)
    }
    fn signal(&self, wf: &str, key: &str, name: &str, attrs: Value, payload: Value) -> anyhow::Result<()> {
        write_outbox("signal", wf, key, Some(name), attrs, payload)
    }
    fn query (&self, wf: &str, key: &str, name: &str, args: Value) -> anyhow::Result<Value> {
        write_outbox("query", wf, key, Some(name), json!({}), args)?;
        Ok(json!({"queued": true}))
    }
}

fn write_outbox(op: &str, wf: &str, key: &str, name: Option<&str>, attrs: Value, body: Value) -> anyhow::Result<()> {
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let dir = PathBuf::from(".nexoscli").join("engine").join("outbox");
    fs::create_dir_all(&dir)?;
    let fname = format!("{}-{}-{}-{}.json",
        now.replace([':', '-','T','Z'], "_"),
        op, wf, sanitize(key)
    );
    let env = json!({
      "op": op, "workflow": wf, "key": key, "name": name,
      "attrs": attrs, "body": body,
      "ts": now
    });
    fs::write(dir.join(fname), serde_json::to_vec_pretty(&env)?)?;
    Ok(())
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

pub fn engine_from(kind: EngineKind) -> Box<dyn WorkflowEngine + Send + Sync> {
    match kind {
        EngineKind::Local => Box::new(LocalEngine),
        EngineKind::TemporalStub => Box::new(TemporalOutboxEngine),
    }
}

Expose it in core lib (nexoscli-core/src/lib.rs):

pub mod engine;
pub use engine::{WorkflowEngine, EngineKind, engine_from};


⸻

2) Task plugin: add --engine and call the seam

We’ll:
	•	add a global --engine choice to task start|stage|end,
	•	compute basic search attributes (good defaults now; you can expand later),
	•	Local: execute your existing gates/hooks (no change in behavior),
	•	TemporalStub: skip hooks, write a start/signal envelope to outbox.

Add the CLI flag in plugins/task/src/lib.rs where you register subcommands:

fn engine_arg() -> clap::Arg {
    clap::Arg::new("engine")
        .long("engine")
        .value_parser(["local","temporal"])
        .default_value("local")
        .help("Execution engine for this action")
}

Add .arg(engine_arg()) to the start, stage, and end subcommands.

Helpers (near the other helpers):

use nexoscli_core::{engine::{engine_from, EngineKind}, Providers};
fn engine_kind_from(matches: &clap::ArgMatches) -> EngineKind {
    match matches.get_one::<String>("engine").map(|s| s.as_str()) {
        Some("temporal") => EngineKind::TemporalStub,
        _ => EngineKind::Local,
    }
}

fn git_context() -> serde_json::Value {
    let mut repo = String::new(); let mut branch = String::new();
    if let Ok(out) = std::process::Command::new("git").args(["rev-parse","--show-toplevel"]).output() {
        if out.status.success() { repo = String::from_utf8_lossy(&out.stdout).trim().to_string(); }
    }
    if let Ok(out) = std::process::Command::new("git").args(["rev-parse","--abbrev-ref","HEAD"]).output() {
        if out.status.success() { branch = String::from_utf8_lossy(&out.stdout).trim().to_string(); }
    }
    serde_json::json!({ "repo": if repo.is_empty(){serde_json::Value::Null}else{serde_json::Value::String(repo)},
                        "branch": if branch.is_empty(){serde_json::Value::Null}else{serde_json::Value::String(branch)} })
}

fn temporal_attrs_for_task(task: &TaskRecord) -> serde_json::Value {
    // **pre-Temporal** mapping (you can formalize in a mapping file later)
    let ctx = git_context();
    let sid = current_session_id();
    serde_json::json!({
      "task_id": task.id, "stage": task.stage, "status": task.status,
      "session_id": sid, "repo": ctx.get("repo"), "branch": ctx.get("branch")
    })
}

Wire it into start, stage, end branches:

// --- START ---
Some(("start", sm)) => {
  let engine_kind = engine_kind_from(sm);
  // ... existing load/update/save of TaskRecord ...
  let eng = engine_from(engine_kind);
  if matches!(engine_kind, EngineKind::TemporalStub) {
      let attrs = temporal_attrs_for_task(&rec);
      let input = serde_json::json!({"action":"start"});
      eng.start("task", &rec.id, attrs, input)?;
      // NOTE: we deliberately **do not** run hooks for Temporal here
  } else {
      // Local engine: behavior unchanged (hooks run on stage transitions)
  }
  // existing ctx.ok output...
}

// --- STAGE ---
Some(("stage", sm)) => {
  let engine_kind = engine_kind_from(sm);
  // ... existing update of rec.stage + save ...
  let eng = engine_from(engine_kind);
  if matches!(engine_kind, EngineKind::TemporalStub) {
      let attrs = temporal_attrs_for_task(&rec);
      let payload = serde_json::json!({"action":"stage_enter","stage": stage});
      eng.signal("task", &rec.id, "stage_enter", attrs, payload)?;
      // **Skip** gates/hooks under Temporal for now (the worker will own them)
  } else {
      // Local: gates/hooks as you already implemented
      let enforce = sm.get_flag("enforce") || std::env::var("NEXOS_TASK_ENFORCE").ok().as_deref()==Some("1");
      if let Some(spec) = load_task_spec() {
          let failures = eval_gates(&stage, &rec.id, &spec);
          if !failures.is_empty() {
              for f in &failures { let _ = ctx.err("task","stage_gate","GATE_FAILED",f,None,1); }
              if enforce { return Err(anyhow::anyhow!("stage '{}' blocked by gates", stage).into()); }
          }
          run_hooks_on_enter(&stage, &rec.id, ctx, &spec);
      }
  }
  // ctx.ok...
}

// --- END ---
Some(("end", sm)) => {
  let engine_kind = engine_kind_from(sm);
  // ... existing end logic ...
  let eng = engine_from(engine_kind);
  if matches!(engine_kind, EngineKind::TemporalStub) {
      let attrs = temporal_attrs_for_task(&rec);
      let payload = serde_json::json!({"action":"end"});
      eng.signal("task", &rec.id, "end", attrs, payload)?;
  }
  // ctx.ok...
}

That’s it: the engine seam is in place. Local keeps working; Temporal mode now produces outbox envelopes you can inspect (and later a real Temporal client will read).

Outbox examples (pretty‑printed):

{
  "op": "start",
  "workflow": "task",
  "key": "TASK-20250902-scaffold-plugin",
  "name": null,
  "attrs": { "task_id": "...", "stage": "planning", "status": "active", "session_id": "SES-...", "repo": "...", "branch": "..." },
  "body": { "action": "start" },
  "ts": "2025-09-02T11:22:33Z"
}


⸻

3) E2E tests (2 small checks)

A) Temporal outbox — nexoscli-bin/tests/engine_outbox.rs

mod helpers; use helpers::*;
use std::{fs, process::Command};

#[test]
fn temporal_engine_writes_outbox() {
    let t = tmp();
    // basic repo
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    // create task and start with temporal engine
    let j = run_json(t.path(), &["task","new","--title","Outbox"]);
    let tid = j["data"]["id"].as_str().unwrap();
    run_ok(t.path(), &["task","start","--task",tid,"--engine","temporal"]);
    // stage with temporal
    run_ok(t.path(), &["task","stage","implementation","--task",tid,"--engine","temporal"]);

    // assert outbox has at least 2 envelopes
    let outbox = t.path().join(".nexoscli/engine/outbox");
    let entries: Vec<_> = fs::read_dir(&outbox).unwrap().collect();
    assert!(entries.len() >= 2, "expected outbox envelopes");
    // sanity: check structure
    let first = fs::read_to_string(entries[0].as_ref().unwrap().path()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert!(v.get("op").is_some());
    assert_eq!(v.get("workflow").unwrap().as_str().unwrap(), "task");
}

B) Local still runs hooks — if you already have a hooks test, nothing else needed. Otherwise:

#[test]
fn local_engine_runs_hooks_unchanged() {
    let t = tmp();
    std::fs::create_dir_all(t.path().join("workflows")).unwrap();
    std::fs::write(t.path().join("workflows/task.yaml"), r#"version:0.1
hooks: { on_enter: { planning: [ { create: ["docs/DDL-<task_id>.md"] } ] } }"#).unwrap();

    let j = run_json(t.path(), &["task","new","--title","Hooks"]);
    let tid = j["data"]["id"].as_str().unwrap();
    run_ok(t.path(), &["task","start","--task",tid,"--engine","local"]);
    run_ok(t.path(), &["task","stage","planning","--task",tid,"--engine","local"]);
    assert!(t.path().join("docs").join(format!("DDL-{}.md", tid)).exists());
}

Run all tests:

cargo test --workspace


⸻

4) Docs quick updates
	•	docs/WORKFLOWS_TASK.md — add:

### Engine selection
You can run `task` transitions either locally or prepare a Temporal envelope:

- Local (default): runs gates/hooks as defined in `workflows/task.yaml`.
- Temporal: `--engine temporal` writes a Start/Signal envelope into `.nexoscli/engine/outbox/` and **does not** run hooks locally.

Examples:

nexoscli task start –task TASK-… –engine temporal
nexoscli task stage verification –engine temporal



	•	docs/PLAYBOOK.md — Add a step: “If you intend to hand off to Temporal later, use --engine temporal to produce an outbox envelope you can inspect or feed to a client.”


