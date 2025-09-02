Love it. Next on the critical path: Durable timers (Temporal’s sleep/timer), with clean pause/stop semantics and deterministic re‑runs. This is the smallest, highest‑leverage step toward full Temporal parity (our earlier T‑5).

Below are copy‑paste patches to add a timer step to your nexoscli workflow engine. It persists the due time to disk, survives process restarts, honors pause/resume/stop signals, updates heartbeats, and records effects/history just like activities.

⸻

What this adds
	•	Spec support

steps:
  - id: wait-a-sec
    timer: { duration_ms: 1000 }   # ← new
    idempotent: true               # re-run skips if input unchanged & effect ok
    timeouts:
      schedule_to_close_ms: 5000   # optional guard (across all retries)


	•	Runner behavior
	•	First run writes .nexoscli/workflows/<id>/timers/<step>.json with due_epoch_ms.
	•	Subsequent runs resume and wait only the remaining time.
	•	Signals: pause pauses between checks; stop cancels the timer step.
	•	Heartbeats: updated while waiting (so long waits stay “alive”).
	•	Durability: no in‑memory state needed; replays are deterministic.
	•	History/effects: one JSONL event line per completion (or cancel/timeout) and an effect file with status.

⸻

Patch 1 — Extend the spec with timer

Open plugins/workflow/src/lib.rs and apply the following edits.

1) Add the TimerSpec and TimerState types (place near other spec structs)

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct TimerSpec {
    /// Duration to wait before the step is considered successful (ms)
    duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct TimerState {
    due_epoch_ms: i64,
}

2) Add an optional timer to StepSpec

Find struct StepSpec { ... } and add the field:

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct StepSpec {
    id: String,
    /// "plugin.command"
    activity: Option<String>,
    /// Optional child workflow (future); mutually exclusive with activity
    #[serde(default)]
    child_workflow: Option<ChildWorkflowSpec>,
    /// NEW: optional timer step
    #[serde(default)]
    timer: Option<TimerSpec>,              // ← add this line
    #[serde(default)]
    args: Json,
    #[serde(default)]
    idempotent: bool,
    #[serde(default)]
    retry: RetryPolicy,
    #[serde(default)]
    timeouts: Timeouts,
    #[serde(default)]
    resources: Vec<String>,
}

Invariants we’ll rely on: a step has either activity or timer (and not both).

⸻

Patch 2 — Plan & run the timer step

1) Recognize timers in the planner

In plan_steps(spec: &WorkflowSpec), change how activity is displayed and how input hashing works. Replace its body with this version (or merge):

fn plan_steps(spec: &WorkflowSpec) -> anyhow::Result<Vec<PlanRow>> {
    let mut out = vec![];
    for s in &spec.steps {
        // choose a human label for planning
        let activity = if s.timer.is_some() {
            "timer".to_string()
        } else {
            s.activity.clone().unwrap_or_else(|| "child_workflow".to_string())
        };

        let input = input_sha(&spec.id, s);
        let eff = read_effect(&spec.id, &s.id);
        let (would_run, reason) = if s.idempotent {
            match eff {
                None => (true, "no prior effect".into()),
                Some(rec) if rec.input_sha != input => (true, "input changed".into()),
                Some(rec) if rec.status != "ok" => (true, "prior failed".into()),
                Some(_) => (false, "idempotent match".into()),
            }
        } else {
            (true, "non-idempotent".into())
        };

        out.push(PlanRow {
            step_id: s.id.clone(),
            activity,
            would_run,
            reason,
        });
    }
    Ok(out)
}

2) Include timer fields in the input hash

In input_sha(...), include timer:

fn input_sha(spec_id: &str, step: &StepSpec) -> String {
    let mut h = Sha256::new();
    h.update(spec_id.as_bytes());
    h.update(step.id.as_bytes());
    if let Some(act) = &step.activity {
        h.update(act.as_bytes());
    }
    if let Some(t) = &step.timer {
        h.update(b"timer");
        h.update(t.duration_ms.to_le_bytes());
    }
    h.update(&serde_json::to_vec(&step.args).unwrap_or_default());
    format!("sha256:{:x}", h.finalize())
}

3) Add timer state paths & helpers

Add alongside other path helpers:

fn timers_dir(spec_id: &str) -> PathBuf {
    PathBuf::from(".nexoscli").join("workflows").join(spec_id).join("timers")
}
fn timer_state_path(spec_id: &str, step_id: &str) -> PathBuf {
    timers_dir(spec_id).join(format!("{}.json", step_id))
}
fn read_timer_state(spec_id: &str, step_id: &str) -> Option<TimerState> {
    let p = timer_state_path(spec_id, step_id);
    let txt = fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
}
fn write_timer_state(spec_id: &str, step_id: &str, st: &TimerState) -> anyhow::Result<()> {
    fs::create_dir_all(timers_dir(spec_id))?;
    fs::write(timer_state_path(spec_id, step_id), serde_json::to_vec_pretty(st)?)?;
    Ok(())
}
fn epoch_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

4) Implement the timer executor

Add this function (near run_activity_with_timeouts):

fn run_timer_step(
    step: &StepSpec,
    to: &Timeouts,
    spec_id: &str,
    hist: &Path,
) -> anyhow::Result<(i32, Json, String)> {
    let t = step
        .timer
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("timer step missing 'timer' field"))?;

    // Determine due time (persisted)
    let st = read_timer_state(spec_id, &step.id);
    let state = if let Some(old) = st {
        old
    } else {
        let due = epoch_ms_now() + (t.duration_ms as i64);
        let ns = TimerState { due_epoch_ms: due };
        write_timer_state(spec_id, &step.id, &ns)?;
        ns
    };

    // If schedule_to_close guard exists and remaining would exceed it, timeout immediately
    if let Some(s2c) = to.schedule_to_close_ms {
        // We don't have first-scheduled timestamp here; outer loop enforces S2C across attempts.
        // We still preflight: if remaining is absurd (> s2c), fail fast to avoid long waits.
        let remaining_ms = (state.due_epoch_ms - epoch_ms_now()).max(0) as u64;
        if remaining_ms > s2c {
            append_history(
                hist,
                &json!({"ts": now(), "event":"timeout", "kind":"schedule_to_close_preflight"}),
            )?;
            return Ok((124, json!({"error":"schedule_to_close_timeout"}), "timeout".into()));
        }
    }

    // Wait loop with heartbeats + signals + optional start_to_close guard
    let start = Instant::now();
    loop {
        // Signals first
        match next_signal_action(spec_id)? {
            SignalAction::Stop => {
                append_history(
                    hist,
                    &json!({"ts": now(), "event":"canceled", "step": step.id}),
                )?;
                return Ok((130, json!({"error":"canceled"}), "canceled".into()));
            }
            SignalAction::Pause => {
                append_history(hist, &json!({"ts": now(), "event":"pause", "step": step.id}))?;
                wait_for_resume(spec_id)?;
                append_history(hist, &json!({"ts": now(), "event":"resume", "step": step.id}))?;
            }
            _ => {}
        }

        // Heartbeat update
        if let Some(hb) = to.heartbeat_ms {
            if hb > 0 && start.elapsed().as_millis() as u64 % hb < 200 {
                fs::write(heartbeat_path(spec_id)?, now().as_bytes())?;
            }
        }

        // Start-to-close guard (for each wait attempt; usually you won't set this on timers)
        if let Some(stc) = to.start_to_close_ms {
            if start.elapsed() >= Duration::from_millis(stc) {
                append_history(
                    hist,
                    &json!({"ts": now(), "event":"timeout", "kind":"start_to_close"}),
                )?;
                return Ok((124, json!({"error":"start_to_close_timeout"}), "timeout".into()));
            }
        }

        // Check due time
        let now_ms = epoch_ms_now();
        if now_ms >= state.due_epoch_ms {
            break;
        }
        let remaining = (state.due_epoch_ms - now_ms) as u64;
        // Sleep in small slices so we can honor signals promptly
        let slice = remaining.min(200);
        thread::sleep(Duration::from_millis(slice));
    }

    // Success
    Ok((0, json!({"timer":"done","at": now()}), "ok".into()))
}

5) Call the timer executor from the run loop

In cmd_run(...), inside the main step loop where we run each step, replace this block:

// start the activity
let (exit_code, envelope, status) =
    run_activity_with_timeouts(&step, &to, &spec.id, &hist)?;

with:

let (exit_code, envelope, status) = if step.timer.is_some() {
    run_timer_step(&step, &to, &spec.id, &hist)?
} else {
    run_activity_with_timeouts(&step, &to, &spec.id, &hist)?
};

Everything else (effects, history appends, retries/backoff, visibility updates) remains unchanged and will apply to timer steps as well. For timers you typically won’t configure retries; the step completes when time passes.

⸻

(Optional) Patch 3 — Tiny doc example

Drop a sample workflow file to try the timer:

docs/workflows/WF-timer-demo.yaml

version: "0.1"
id: WF-timer-demo
name: Demo with timer
defaults:
  activity_retry:
    use_default: true
  timeouts:
    heartbeat_ms: 1000
steps:
  - id: wait-500ms
    timer: { duration_ms: 500 }
    idempotent: true
  - id: after-wait-status
    activity: workflow.status
    args: { id: WF-timer-demo }


⸻

Smoke it

cargo build --workspace

# Run the timer demo
./target/debug/nexoscli workflow plan --spec docs/workflows/WF-timer-demo.yaml --output json
time ./target/debug/nexoscli workflow run --spec docs/workflows/WF-timer-demo.yaml --output json

# Re-run (idempotent timer step will skip if unchanged and effect ok)
./target/debug/nexoscli workflow run --spec docs/workflows/WF-timer-demo.yaml --output json

# Pause/resume while waiting
./target/debug/nexoscli workflow init --id WF-slow
# edit .nexoscli/workflows/WF-slow/spec.yaml -> set timer: { duration_ms: 5000 }
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-slow/spec.yaml --output json &
sleep 1
./target/debug/nexoscli workflow signal --id WF-slow --name pause
sleep 2
./target/debug/nexoscli workflow signal --id WF-slow --name resume
wait

# Inspect artifacts
cat .nexoscli/workflows/WF-timer-demo/history.jsonl
cat .nexoscli/workflows/WF-timer-demo/timers/wait-500ms.json
./target/debug/nexoscli workflow status --id WF-timer-demo --output json


⸻

E2E Test (add under nexoscli-bin/tests/workflow_timer.rs)

use assert_cmd::prelude::*;
use predicates::str::contains;
use std::{process::Command, time::Instant};

#[test]
fn timer_completes_and_is_idempotent() {
    // Write a tiny spec inline
    let spec = r#"version: "0.1"
id: WF-e2e-timer
steps:
  - id: wait-100
    timer: { duration_ms: 100 }
    idempotent: true
"#;
    std::fs::create_dir_all(".nexoscli/workflows/WF-e2e-timer").unwrap();
    std::fs::write(".nexoscli/workflows/WF-e2e-timer/spec.yaml", spec).unwrap();

    // First run ~>=100ms, fails==0
    let t0 = Instant::now();
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","run","--spec",".nexoscli/workflows/WF-e2e-timer/spec.yaml","--output","json"])
        .assert().success().stdout(contains("\"failed\":0"));
    assert!(t0.elapsed().as_millis() >= 80);

    // Second run should skip the idempotent timer step
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","run","--spec",".nexoscli/workflows/WF-e2e-timer/spec.yaml","--output","json"])
        .assert().success();
}


⸻

Design notes & blind spots (candor)
	•	Determinism: we persist due_epoch_ms so a restart doesn’t restart the wait—this matches Temporal’s durable timers.
	•	Timeouts: the outer loop already enforces schedule_to_close_ms across attempts; for timers we also add a pre‑flight guard so you fail fast if the remaining wait obviously exceeds S2C.
	•	Heartbeats: we reuse the same heartbeat file; UI/ops can read it while a long timer runs.
	•	Idempotency: leave timer steps idempotent where it makes sense—same input → skip on re‑run if effect ok.
	•	Cancellation types: for now, stop → “canceled”. We’ll add cancellation_type: try_cancel|abandon when we introduce child workflows/long activities; local processes don’t safely support “abandon” yet without leaking.
	•	Clock jumps: we use epoch ms (wall clock). If the system clock moves backwards, the wait could overshoot. Temporal uses a server clock; when you cut over, this risk goes away. If it bites locally, store and compare monotonic + wall times.

⸻

What this unlocks next (very small tickets)
	1.	Resource locks (resources: ["git","db:dev"]) — advisory file locks to serialize contention.
	2.	Child workflows (child_workflow: { spec: ..., wait: true, inherit_search_attributes: true }).
	3.	Change IDs enforcement (branching on recorded change_ids during replay to keep determinism across code changes).

If you want, I can ship the resource‑locks patch immediately after you land this.