Yes—next two pieces on the Temporal path are:
	1.	Resource locks & concurrency guards (serialize “git”, “db:dev”, etc.)
	2.	Child workflows (recursive composition: parent ↔ child run, wait: true|false, SA inheritance)

Below are drop‑in patches for your plugins/workflow crate (still named nexoscli). They build on your current file (signals/queries/heartbeats/visibility/timeouts/retries/timers).

⸻

1) Resource locks (advisory, durable)

What you get
	•	Step‑level resources: ["git","db:dev"]
	•	Deterministic sorted acquisition to avoid deadlocks
	•	Best‑effort stale detection with lock_stale_ms (default 10m)
	•	Acquire timeout per step with lock_timeout_ms (default 5s)
	•	Lock files under .nexoscli/locks/<resource>.json with owner metadata
	•	Locks are released automatically at the end of the step (even on failure)

Spec (no breaking changes)

steps:
  - id: compile
    activity: eng.build
    args: { release: true }
    resources: ["git"]          # ← serialize access to repo
    lock_timeout_ms: 5000       # optional (default 5000)
    lock_stale_ms: 600000       # optional (default 600000 = 10m)

Patch A — add fields to StepSpec

In plugins/workflow/src/lib.rs, extend StepSpec:

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct StepSpec {
    id: String,
    activity: Option<String>,
    #[serde(default)]
    child_workflow: Option<ChildWorkflowSpec>,
    #[serde(default)]
    timer: Option<TimerSpec>,
    #[serde(default)]
    args: Json,
    #[serde(default)]
    idempotent: bool,
    #[serde(default)]
    retry: RetryPolicy,
    #[serde(default)]
    timeouts: Timeouts,
    #[serde(default)]
    resources: Vec<String>,          // existing
    // NEW:
    #[serde(default)]
    lock_timeout_ms: Option<u64>,    // per-step acquire timeout
    #[serde(default)]
    lock_stale_ms: Option<u64>,      // consider lock stale after this
}

Patch B — lock helpers (add near other path helpers)

fn locks_dir() -> PathBuf {
    PathBuf::from(".nexoscli").join("locks")
}
fn lock_path(resource: &str) -> PathBuf {
    locks_dir().join(format!("{resource}.json"))
}

fn acquire_locks_for_step(spec_id: &str, step: &StepSpec) -> anyhow::Result<Option<LockGuard>> {
    if step.resources.is_empty() { return Ok(None); }
    let timeout = step.lock_timeout_ms.unwrap_or(5_000);
    let stale   = step.lock_stale_ms.unwrap_or(600_000);
    let mut res = step.resources.clone();
    res.sort(); res.dedup();
    fs::create_dir_all(locks_dir())?;

    let mut acquired = Vec::new();
    for r in res {
        if try_acquire_lock(&r, timeout, stale, spec_id, &step.id)? {
            acquired.push(r);
        } else {
            // release what we got and fail
            for a in &acquired { let _ = release_lock(a, spec_id, &step.id); }
            anyhow::bail!("lock_timeout");
        }
    }
    Ok(Some(LockGuard { spec_id: spec_id.to_string(), step_id: step.id.clone(), resources: acquired }))
}

fn try_acquire_lock(resource: &str, timeout_ms: u64, stale_ms: u64, spec_id: &str, step_id: &str) -> anyhow::Result<bool> {
    use std::time::{Duration, Instant};
    let p = lock_path(resource);
    let start = Instant::now();
    loop {
        // try atomic create
        match fs::OpenOptions::new().create_new(true).write(true).open(&p) {
            Ok(mut f) => {
                let body = json!({
                    "resource": resource,
                    "spec_id": spec_id,
                    "step_id": step_id,
                    "pid": std::process::id(),
                    "acquired_at": now(),
                    "updated_at": now()
                });
                use std::io::Write;
                f.write_all(serde_json::to_vec_pretty(&body)?.as_slice())?;
                return Ok(true);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // stale check
                if let Ok(txt) = fs::read_to_string(&p) {
                    if let Ok(v) = serde_json::from_str::<Json>(&txt) {
                        let updated = v.get("updated_at").and_then(|x| x.as_str()).unwrap_or("");
                        if let Ok(t) = time::OffsetDateTime::parse(updated, &time::format_description::well_known::Rfc3339) {
                            let age_ms = (OffsetDateTime::now_utc() - t).whole_milliseconds().max(0) as u64;
                            if age_ms > stale_ms {
                                let _ = fs::remove_file(&p); // best effort
                                continue; // try again
                            }
                        }
                    }
                }
                if start.elapsed() >= Duration::from_millis(timeout_ms) { return Ok(false); }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn release_lock(resource: &str, spec_id: &str, step_id: &str) -> anyhow::Result<()> {
    let p = lock_path(resource);
    // best-effort owner check
    if let Ok(txt) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<Json>(&txt) {
            let ok_owner =
                v.get("spec_id").and_then(|x| x.as_str()) == Some(spec_id) &&
                v.get("step_id").and_then(|x| x.as_str()) == Some(step_id);
            if ok_owner { let _ = fs::remove_file(&p); return Ok(()); }
        }
    }
    Ok(()) // ignore if not ours
}

struct LockGuard { spec_id: String, step_id: String, resources: Vec<String> }
impl Drop for LockGuard {
    fn drop(&mut self) {
        for r in &self.resources { let _ = release_lock(r, &self.spec_id, &self.step_id); }
    }
}

Patch C — acquire/release around every step

In the main run loop inside cmd_run, right before you execute a step (timer/activity/child), acquire locks:

// acquire resource locks (if any)
let _locks = match acquire_locks_for_step(&spec.id, &step) {
    Ok(g) => g, // Some(LockGuard) or None
    Err(e) => {
        // record lock_timeout or failure as an effect and continue
        let finished_at = now();
        let rec = EffectRecord {
            spec_id: spec.id.clone(),
            step_id: step.id.clone(),
            input_sha: input_sha.clone(),
            status: if format!("{e:?}").contains("lock_timeout") { "timeout".into() } else { "failed".into() },
            started_at: finished_at.clone(),
            finished_at: finished_at.clone(),
            exit_code: 128,
            result_digest: None,
            last_envelope: json!({"error":"lock_acquire_failed","detail": format!("{e}")}),
        };
        write_effect(&spec.id, &step.id, &rec)?;
        append_history(&hist, &json!({"ts": finished_at, "step": step.id, "activity": step.activity, "status": rec.status}))?;
        vis.failed += 1;
        vis.updated_at = now();
        write_visibility(&spec.id, &vis)?;
        continue;
    }
};

_locks is intentionally unused—its Drop releases locks automatically at the end of the scope, regardless of success/failure.

⸻

2) Child workflows (recursive composition)

What you get
	•	Step can run another workflow spec (“child”)
	•	wait: true → parent blocks until child completes; parent step status mirrors child exit status
	•	wait: false → fire‑and‑forget; parent step returns success after spawn
	•	inherit_search_attributes: true → parent’s SA are upserted into the child after it starts (simple, durable approach without mutating child spec)

Spec

steps:
  - id: run-child
    child_workflow:
      spec: docs/workflows/feature-slice.yaml
      wait: true
      inherit_search_attributes: true
    idempotent: false

Patch D — (If not already present) keep the ChildWorkflowSpec struct

You already have:

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ChildWorkflowSpec {
    spec: String,
    #[serde(default)]
    wait: bool,
    #[serde(default)]
    inherit_search_attributes: bool,
}

Patch E — label child in planner

Replace the activity label logic in plan_steps:

let activity = if s.timer.is_some() {
    "timer".to_string()
} else if let Some(c) = &s.child_workflow {
    // Try to show child id if parseable
    match fs::read_to_string(&c.spec) {
        Ok(txt) => match serde_yaml::from_str::<WorkflowSpec>(&txt) {
            Ok(cs) => format!("child:{}", cs.id),
            Err(_) => "child_workflow".into(),
        },
        Err(_) => "child_workflow".into(),
    }
} else {
    s.activity.clone().unwrap_or_else(|| "child_workflow".to_string())
};

Patch F — child runner

Add this executor (near run_activity_with_timeouts / run_timer_step):

fn run_child_workflow(
    step: &StepSpec,
    parent: &WorkflowSpec,
    hist: &Path,
) -> anyhow::Result<(i32, Json, String)> {
    let c = step.child_workflow.as_ref().ok_or_else(|| anyhow::anyhow!("missing child_workflow spec"))?;
    let child_spec_path = &c.spec;

    // discover child id
    let child_id = match fs::read_to_string(child_spec_path)
        .ok()
        .and_then(|txt| serde_yaml::from_str::<WorkflowSpec>(&txt).ok())
        .map(|cs| cs.id)
    {
        Some(id) => id,
        None => "UNKNOWN".into(),
    };

    append_history(hist, &json!({"ts": now(), "event":"child_start", "child_spec": child_spec_path, "child_id": child_id, "wait": c.wait }))?;

    let exe = std::env::current_exe()?;
    if c.wait {
        let out = Proc::new(exe)
            .args(["workflow","run","--spec", child_spec_path, "--output","json"])
            .output()?;
        let code = out.status.code().unwrap_or(-1);
        let env: Json = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| json!({"stdout": String::from_utf8_lossy(&out.stdout)}));

        // upsert parent's SA if requested
        if c.inherit_search_attributes && child_id != "UNKNOWN" {
            let mut kvs = vec![];
            for (k, v) in &parent.run.search_attributes {
                let val = v.as_str().unwrap_or_else(|| &v.to_string()).to_string();
                kvs.push(format!("{}={}", k, val));
            }
            if !kvs.is_empty() {
                let _ = Proc::new(exe)
                    .args(["workflow","upsert-sa","--id",&child_id,"--attrs"])
                    .args(kvs)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }

        append_history(hist, &json!({"ts": now(), "event":"child_done", "child_id": child_id, "exit_code": code }))?;
        Ok((code, json!({"child_id": child_id, "result": env}), if code==0 { "ok".into() } else { "failed".into() }))
    } else {
        // fire-and-forget
        let _child = Proc::new(exe)
            .args(["workflow","run","--spec", child_spec_path, "--output","json"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // SA inheritance after spawn is racy; we skip for fire-and-forget in v0.
        Ok((0, json!({"child_id": child_id, "spawned": true}), "ok".into()))
    }
}

Patch G — dispatch to child/timer/activity

In the step execution branch inside cmd_run, replace:

let (exit_code, envelope, status) =
    run_activity_with_timeouts(&step, &to, &spec.id, &hist)?;

with:

let (exit_code, envelope, status) = if step.timer.is_some() {
    run_timer_step(&step, &to, &spec.id, &hist)?
} else if step.child_workflow.is_some() {
    run_child_workflow(&step, &spec, &hist)?
} else {
    run_activity_with_timeouts(&step, &to, &spec.id, &hist)?
};


⸻

Smoke checks

cargo build --workspace

# 1) Locks
# Create a spec that grabs "git" for 3s while running a timer.
cat > .nexoscli/workflows/WF-lock-demo.yaml <<'YAML'
version: "0.1"
id: WF-lock-demo
steps:
  - id: hold-git
    timer: { duration_ms: 3000 }
    resources: ["git"]
    lock_timeout_ms: 2000
    idempotent: false
YAML

# Start it in the background
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-lock-demo.yaml --output json &

# Try to run another spec that also needs "git" with a short lock timeout; it should fail fast with timeout
cat > .nexoscli/workflows/WF-lock-contender.yaml <<'YAML'
version: "0.1"
id: WF-lock-contender
steps:
  - id: also-git
    timer: { duration_ms: 100 }
    resources: ["git"]
    lock_timeout_ms: 500
    idempotent: false
YAML

./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-lock-contender.yaml --output json || echo "expected contention"
ls -l .nexoscli/locks
wait

# 2) Child workflow
# Child
cat > docs/workflows/WF-child.yaml <<'YAML'
version: "0.1"
id: WF-child
steps:
  - id: wait-100
    timer: { duration_ms: 100 }
YAML

# Parent
cat > docs/workflows/WF-parent.yaml <<'YAML'
version: "0.1"
id: WF-parent
run:
  search_attributes: { task_queue: "nexos", owner: "christian" }
steps:
  - id: call-child
    child_workflow:
      spec: docs/workflows/WF-child.yaml
      wait: true
      inherit_search_attributes: true
YAML

./target/debug/nexoscli workflow run --spec docs/workflows/WF-parent.yaml --output json
./target/debug/nexoscli workflow status --id WF-child --output json  # should show SA owner=christian


⸻

E2E tests (add under nexoscli-bin/tests/workflow_locks_child.rs)

use assert_cmd::prelude::*;
use predicates::str::contains;
use std::{process::Command, thread, time::Duration};

#[test]
fn lock_serializes_and_child_runs() {
    // lock holder
    let holder = r#"version: "0.1"
id: WF-lock-holder
steps:
  - id: hold
    timer: { duration_ms: 1500 }
    resources: ["git"]
    lock_timeout_ms: 2000
    idempotent: false
"#;
    std::fs::create_dir_all(".nexoscli/workflows/WF-lock-holder").unwrap();
    std::fs::write(".nexoscli/workflows/WF-lock-holder/spec.yaml", holder).unwrap();

    // contender (short timeout)
    let contender = r#"version: "0.1"
id: WF-lock-contender
steps:
  - id: need
    timer: { duration_ms: 10 }
    resources: ["git"]
    lock_timeout_ms: 300
    idempotent: false
"#;
    std::fs::create_dir_all(".nexoscli/workflows/WF-lock-contender").unwrap();
    std::fs::write(".nexoscli/workflows/WF-lock-contender/spec.yaml", contender).unwrap();

    // run holder in background
    let mut bg = Command::cargo_bin("nexoscli").unwrap();
    bg.args(["workflow","run","--spec",".nexoscli/workflows/WF-lock-holder/spec.yaml","--output","json"]);
    let mut child = bg.spawn().unwrap();

    // small delay to ensure lock acquired
    thread::sleep(Duration::from_millis(200));

    // contender should fail (exit!=0)
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","run","--spec",".nexoscli/workflows/WF-lock-contender/spec.yaml","--output","json"])
        .assert().failure();

    let _ = child.wait();
}

#[test]
fn child_wait_true_propagates() {
    let child = r#"version: "0.1"
id: WF-child-e2e
steps:
  - id: wait
    timer: { duration_ms: 50 }
"#;
    std::fs::create_dir_all("docs/workflows").unwrap();
    std::fs::write("docs/workflows/WF-child.yaml", child).unwrap();

    let parent = r#"version: "0.1"
id: WF-parent-e2e
run:
  search_attributes: { owner: "christian" }
steps:
  - id: call-child
    child_workflow:
      spec: docs/workflows/WF-child.yaml
      wait: true
      inherit_search_attributes: true
"#;
    std::fs::write("docs/workflows/WF-parent.yaml", parent).unwrap();

    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","run","--spec","docs/workflows/WF-parent.yaml","--output","json"])
        .assert().success().stdout(contains("\"failed\":0"));

    // child should exist and have status
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","status","--id","WF-child-e2e","--output","json"])
        .assert().success();
}


⸻

Acceptance criteria (for this drop)
	•	Step can declare resources and runs exclusively; lock files are created/released.
	•	If a lock can’t be acquired within lock_timeout_ms, the step records an effect with status="timeout" and the run continues with failed += 1.
	•	Child steps with wait: true propagate child’s exit code; with wait: false return success after spawn.
	•	With inherit_search_attributes: true, the child’s visibility shows the parent’s SAs (upserted).
	•	All existing features (signals/queries/timeouts/visibility/timers) still work.

⸻

What this unlocks next (tiny follow‑ups) add to ROADMAP_NEXT.md as suggestions
	1.	Change‑IDs enforcement: When a spec lists run.change_ids, record them (you already do), and branch workflow code only if the change‑id is present in history (Temporal‑style workflow versioning).
	2.	Cancellation types: per‑step cancellation_type: try_cancel|abandon (mirrors Temporal).
	3.	Fire‑and‑forget SA inheritance: add a small retry to upsert-sa until child visibility appears.
	4.	Lock heartbeats: periodically touch lock JSON updated_at alongside your heartbeat loop (only needed if you set lock_stale_ms short).
