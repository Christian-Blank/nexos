# Workflow Plugin (Phase 18)

This document describes the workflow plugin features delivered in Phase 18: signals, queries, heartbeats, timeouts/retries (Temporal-style), and visibility with listing and upsert of search attributes.

The workflow plugin executes YAML specs with idempotent activities and records effects, history, and visibility under `.nexoscli/`.

## CLI Commands

- Initialize a spec
  ./target/debug/nexoscli workflow init --id WF-demo

- Plan which steps would run (idempotency-aware)
  ./target/debug/nexoscli workflow plan --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

- Run a workflow
  ./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

- Status (effects, history, visibility)
  ./target/debug/nexoscli workflow status --id WF-demo --output json

- Signals (file-based mailbox)
  ./target/debug/nexoscli workflow signal --id WF-demo --name pause --payload "{}"
  ./target/debug/nexoscli workflow signal --id WF-demo --name resume
  ./target/debug/nexoscli workflow signal --id WF-demo --name stop

- Query (runtime state)
  ./target/debug/nexoscli workflow query --id WF-demo --name status --output json

- Visibility (list, filter by search attributes and fields)
  ./target/debug/nexoscli workflow list --filter "task_queue=nexos AND state=Running" --output json

- Upsert search attributes (repeatable --attrs)
  ./target/debug/nexoscli workflow upsert-sa --id WF-demo --attrs owner=christian --attrs priority=high --output json

## Spec Schema (additions in Phase 18)

Top-level:
- version: "0.1"
- id: "<wf-id>"
- name: optional
- defaults:
  - activity_retry: RetryPolicy
  - timeouts: Timeouts
- run:
  - memo: Json map (string->string|number|bool|stringified object)
  - search_attributes: Json map (string->string)
  - build_id: Option string
  - change_ids: Vec string
- steps: array of StepSpec

StepSpec:
- id: string
- activity: "plugin.command" (multi-segment supported, e.g. "dev.plugins.check")
- args: JSON object with simple mapping to flags
- idempotent: bool
- retry: RetryUnion
  - SimpleRetry (legacy): { max_attempts: u32, backoff_ms: u64 }
  - RetryPolicy (Temporal-ish):
    - use_default: bool
    - initial_interval_ms: Option<u64>
    - backoff_coefficient: Option<f64>
    - maximum_interval_ms: Option<u64>
    - maximum_attempts: Option<u32> (0=unlimited)
- timeouts:
  - start_to_close_ms: Option<u64> (per attempt)
  - schedule_to_close_ms: Option<u64> (across retries)
  - heartbeat_ms: Option<u64>
- resources: Vec<String> (reserved for future use)

## Execution Semantics

- Idempotency and Effects
  - Each step writes an EffectRecord to .nexoscli/effects/<wf-id>/<step-id>.json including input_sha, status ("ok"|"failed"|"timeout"|"canceled"), exit_code, and last envelope.
  - plan is idempotency-aware and may skip steps whose prior effect matches (ok + same input).

- History
  - JSONL appended to .nexoscli/workflows/<wf-id>/history.jsonl with run events (skip/run attempt/timeout/pause/resume/stop).

- Signals (Mailbox)
  - Files queued under .nexoscli/workflows/<wf-id>/signals/<ts>_<name>.json.
  - Before each step: coalesce signals with priority Stop > Pause > Resume; "pause" blocks until a "resume" signal arrives; "stop" cancels the run.

- Heartbeats and Timeouts
  - While an activity runs, the runner writes a "heartbeat" file periodically when heartbeat_ms is set.
  - start_to_close_ms: per-attempt timeout; schedule_to_close_ms: across-retries ceiling for the step.

- Retry Policy
  - Temporal-like backoff: next interval = clamp(initial_interval * coefficient^(attempt-1), maximum_interval).
  - maximum_attempts=0 or None → unlimited (bounded by schedule_to_close_ms if set).

## Visibility Index

- File: .nexoscli/workflows/<wf-id>/visibility.json
- Fields: id, state (Running|Completed|Failed|Canceled|Paused), memo, search_attributes, build_id, used_change_ids, started_at, updated_at, last_step, attempts, ran, skipped, failed.

- Listing and Filtering:
  - workflow list [--filter "key=value AND other=value"]
  - Matches against visibility.id, visibility.state, visibility.build_id, then search_attributes, then memo (string values only).

- Upsert of Search Attributes:
  - workflow upsert-sa --id <id> --attrs key=value [--attrs key2=value2...]

## Quick Example

Initialize, run, signal, and query:
- ./target/debug/nexoscli workflow init --id WF-demo
- ./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --output json
- ./target/debug/nexoscli workflow signal --id WF-demo --name pause
- ./target/debug/nexoscli workflow signal --id WF-demo --name resume
- ./target/debug/nexoscli workflow query --id WF-demo --name status --output json

Filter by default SA and upsert a new attribute:
- ./target/debug/nexoscli workflow list --filter "task_queue=nexos" --output json
- ./target/debug/nexoscli workflow upsert-sa --id WF-demo --attrs owner=christian --output json
- ./target/debug/nexoscli workflow list --filter "owner=christian" --output json

## Timer Step (Phase 19)

This phase adds a durable timer step with deterministic re-runs, pause/resume/stop semantics, and heartbeats while waiting.

Spec additions:
- StepSpec supports timers via:
  - timer: { duration_ms: u64 }
  - activity becomes optional (exactly one of activity or timer must be set)

Example:
steps:
  - id: wait-a-second
    timer: { duration_ms: 1000 }
    idempotent: true

Runner behavior:
- Durability: On first run, the engine persists due time to .nexoscli/workflows/<id>/timers/<step>.json and uses it on subsequent runs, waiting only the remaining time.
- Signals: pause blocks the wait (resume continues), stop cancels the step and the run (canceled). Signals are processed at short intervals for responsiveness.
- Heartbeats: If heartbeat_ms is configured, the runner updates the workflow heartbeat file while waiting so long timers are “alive”.
- Timeouts: start_to_close_ms applies per wait attempt; schedule_to_close_ms bounds the total time window across attempts. A preflight guard will fail fast if remaining wait obviously exceeds schedule_to_close_ms.
- Idempotency and planning: input_sha includes a “timer” marker + duration_ms; planner labels timer steps as "timer". If idempotent and a prior effect with matching input_sha has status=ok, the step is skipped.
- History/effects: Completion (or timeout/cancel) writes a history entry and an EffectRecord just like activities.

Try the demo:
- See docs/workflows/WF-timer-demo.yaml
- Run:
  ./target/debug/nexoscli workflow plan --spec docs/workflows/WF-timer-demo.yaml --output json
  time ./target/debug/nexoscli workflow run  --spec docs/workflows/WF-timer-demo.yaml --output json

## Notes

- The CLI contracts are additive in this phase; existing specs remain valid.
- Effects/history/visibility are local files enabling deterministic replay and visibility.
- JSON envelopes include task context automatically when an active task exists (see PLAYBOOK and Task CLI).
