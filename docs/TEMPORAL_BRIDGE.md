# Temporal Bridge (T‑0)

This document describes how to run the minimal Python-based bridge that consumes the workflow plugin’s Temporal Outbox envelopes and forwards them to a Temporal server. No Rust SDK or network logic was added to nexoscli; the bridge is entirely optional and lives under docs/.

Scope and goals:
- Keep changes within docs/** and tests/** (workflow-plugin scope).
- Worker and drainer are small Python processes:
  - adapters/temporal-worker/worker.py — Temporal worker hosting a NexosRunner workflow and a run_cli_activity
  - adapters/temporal-worker/drain_outbox.py — Outbox drainer (Temporal client)
- When the CLI is invoked with --engine temporal, envelopes are written to .nexoscli/engine/outbox/. The drainer turns them into a StartWorkflow and Signals for the Temporal worker.

References
- Contract: see docs/WORKFLOWS_PLUGIN.md “Temporal Outbox (Phase 22 — T‑1)”
- Outbox dir: .nexoscli/engine/outbox/
- Envelope shape:
  { op, workflow, run_id, key, name?, attrs { build_id, change_ids, spec_sha256, search_attributes, memo }, body, ts }
- Supported ops:
  - start/workflow.start
  - activity.schedule
  - timer.schedule
  - child.start
  - signal/workflow.signal


## Install

Requirements:
- Python 3.11 or 3.12
- Temporal server reachable (local dev: 127.0.0.1:7233; namespace “default”)
- nexoscli built and available (so activities can shell out)

Two ways to run the Python project under adapters/temporal-worker:

Option A: uv (recommended for isolated runs)
- Using uv without a global install:
  uv run --project adapters/temporal-worker --help

- Or create a virtual env and install:
  uv pip install -e adapters/temporal-worker

Option B: pip (system/python venv)
- python -m venv .venv && source .venv/bin/activate
- pip install -e adapters/temporal-worker


## Environment

- TEMPORAL_ADDRESS (default: 127.0.0.1:7233)
- TEMPORAL_NAMESPACE (default: default)
- TEMPORAL_TASK_QUEUE (default: nexos)
- TEMPORAL_USE_SA (default: 1)
  - If set to 0, the drainer will drop search_attributes to avoid failures on clusters requiring pre-registered fields.
- NEXOS_BUILD_ID (optional; reserved for future versioning)
- NEXOSCLI_BIN (optional; path to nexoscli binary if not on PATH)

Example (bash):
export TEMPORAL_ADDRESS=127.0.0.1:7233
export TEMPORAL_NAMESPACE=default
export TEMPORAL_TASK_QUEUE=nexos
export TEMPORAL_USE_SA=1


## Run the Worker

Start the Temporal worker that hosts the NexosRunner workflow and run_cli_activity.

- With uv:
  uv run --project adapters/temporal-worker nexos-temporal-worker

- With pip/venv:
  python -m docs.temporal_worker.worker
  # or if installed:
  nexos-temporal-worker

Worker behavior:
- Workflow class NexosRunner:
  - Signals:
    - activitySchedule(body) → enqueue op: activity.schedule
    - timerSchedule(body) → enqueue op: timer.schedule
    - childStart(body) → enqueue op: child.start
    - pause(), resume(), stop()
  - Query:
    - status() → { paused, stopped, processed, last_event }
  - Run loop:
    - Processes queue FIFO; honors pause/resume/stop
  - Activities:
    - run_cli_activity(activity_name, args) executes:
      nexoscli <plugin> <command...> --output json ...


## Produce Outbox Envelopes

Use the existing CLI with --engine temporal. Example quick spec:

mkdir -p .nexoscli/workflows/WF-demo
cat > .nexoscli/workflows/WF-demo/spec.yaml <<YAML
version: "0.1"
id: WF-demo
run:
  build_id: "build-T0"
  change_ids: ["CH_T0"]
  memo: {}
  search_attributes: {"task_queue":"nexos"}
steps:
  - id: a1
    activity: "dev.plugins.check"
    args: {}
    idempotent: true
    retry: { max_attempts: 1, backoff_ms: 100 }
    timeouts: {}
    resources: []
  - id: t1
    timer: { duration_ms: 250 }
YAML

# Emit envelopes
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --engine temporal --output json


## Drain the Outbox

In another terminal:

- With uv:
  uv run --project adapters/temporal-worker nexos-temporal-drain

- With pip/venv:
  python -m docs.temporal_worker.drain_outbox
  # or if installed:
  nexos-temporal-drain

Flags:
- --dry-run → parse and map envelopes without calling Temporal
- --outbox PATH → override the outbox directory (default: .nexoscli/engine/outbox)

Behavior:
- start/workflow.start → start NexosRunner if not already running (WorkflowId = envelope.workflow)
- activity.schedule → signal activitySchedule(body)
- timer.schedule → signal timerSchedule(body)
- child.start → signal childStart(body)
- signal/workflow.signal → forward signal “name” with body
- Processed files are moved to .nexoscli/engine/outbox/processed/


## Signals

From nexoscli:
- Pause:
  ./target/debug/nexoscli workflow signal --id WF-demo --name pause --engine temporal --output json

- Resume:
  ./target/debug/nexoscli workflow signal --id WF-demo --name resume --engine temporal --output json

- Stop:
  ./target/debug/nexoscli workflow signal --id WF-demo --name stop --engine temporal --output json


## Observability

- Temporal UI:
  - Observe workflow “WF-demo”
  - Activity execution of dev.plugins.check (shells out to nexoscli)
  - Timer sleeps
  - Child workflows (if signaled)

- Attributes & memo:
  - When TEMPORAL_USE_SA=1 (default), search_attributes from the envelope are sent as Temporal Search Attributes.
  - Memo includes: run_id (nexos_run_id), build_id, change_ids, spec_sha256 plus any memo provided in the envelope.


## Limitations

- Minimal bridge uses Temporal WorkflowId = workflow spec id; concurrent runs per id are not supported.
  - The envelope run_id is propagated in memo for traceability.
- Retry policies and advanced timeouts are not mapped to Temporal in this minimal version; only activity start_to_close_ms is mapped.
- Search attribute schemas may need pre-registration in some clusters; set TEMPORAL_USE_SA=0 to disable.


## Troubleshooting

- Activity fails with “nexoscli not found”
  - Ensure nexoscli is built and on PATH, or set NEXOSCLI_BIN to an absolute path.
- No envelopes processed
  - Verify outbox path: .nexoscli/engine/outbox/
  - Run drainer with --dry-run to inspect mapping without Temporal calls.
- Search attributes errors (schema)
  - Set TEMPORAL_USE_SA=0 to drop SAs.
- Connection errors
  - Check TEMPORAL_ADDRESS and TEMPORAL_NAMESPACE.
  - Confirm Temporal server is reachable locally or over the network.


## Smoke Procedure (end-to-end)

1) Build and doctor:
   cargo build --workspace
   ./target/debug/nexoscli dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph

2) Worker:
   uv run --project adapters/temporal-worker nexos-temporal-worker

3) Outbox:
   ./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --engine temporal --output json

4) Drain:
   uv run --project adapters/temporal-worker nexos-temporal-drain

5) Signal:
   ./target/debug/nexoscli workflow signal --id WF-demo --name pause --engine temporal --output json
   ./target/debug/nexoscli workflow signal --id WF-demo --name resume --engine temporal --output json

6) Observe in Temporal UI; note that processed envelopes are moved to .nexoscli/engine/outbox/processed/.
