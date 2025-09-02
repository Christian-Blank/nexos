Awesome—time to ship T‑0: minimal Temporal bridge so you can “flip the switch” with zero churn to your specs.

We’ll add a tiny outbox → Temporal bridge plus a worker that executes steps (activity, timer, child) using the same contracts you already standardized. This keeps local flow unchanged and gives you a clean path to run the exact same workflow specs on Temporal.

Below are drop‑in files and small docs/tests. Paste them in, run the smoke, and you’re Temporal‑ready.

⸻

What we’re adding in this step

Two small Python processes (kept outside of Rust core, optional at runtime):
	1.	temporal_worker/worker.py — Temporal worker
	•	Workflow: NexosRunner (signals: activitySchedule, timerSchedule, childStart, pause/resume/stop)
	•	Activity: run_cli_activity (shells out to nexoscli using your stable JSON contracts)
	•	Uses Worker Build ID (NEXOS_BUILD_ID env) for versioning sets
	2.	temporal_worker/drain_outbox.py — Outbox drainer (client)
	•	Reads .nexoscli/engine/outbox/*.json (your T‑1 envelopes)
	•	Maps:
	•	workflow.start → StartWorkflow (memo + search attributes)
	•	activity.schedule → signal activitySchedule
	•	timer.schedule → signal timerSchedule
	•	child.start → signal childStart
	•	workflow.signal → forwards signal unchanged
	•	Moves processed files to .nexoscli/engine/outbox/processed/

No SDK in Rust. When you pass --engine temporal, the CLI only writes envelopes. Running the worker/drainer is a separate step you turn on when you want.

⸻

1) Add Python project (uv-friendly)

temporal_worker/pyproject.toml

[project]
name = "nexos-temporal-bridge"
version = "0.1.0"
requires-python = ">=3.11,<3.13"
dependencies = [
  "temporalio>=1.7.0",
]

[project.scripts]
nexos-temporal-worker = "worker:main"
nexos-temporal-drain  = "drain_outbox:main"

Pinned to Python 3.11/3.12 for broad Temporal SDK support. You can try 3.13 later once the SDK certifies it.

⸻

2) Worker (workflow + activity)

temporal_worker/worker.py

import os, sys, asyncio, json, shlex
from datetime import timedelta
from typing import Any, Dict, List, Optional, Tuple

from temporalio import activity, workflow
from temporalio.client import Client
from temporalio.worker import Worker

# ---------- Activity: run_cli_activity ---------------------------------------

@activity.defn
async def run_cli_activity(payload: Dict[str, Any]) -> Dict[str, Any]:
    """
    payload:
      {
        "workflow_id": "<spec_id>",
        "step_id": "s1",
        "activity": "plugin.command",
        "args": {...},                 # JSON mapped to flags
        "idempotent": true/false,
        "retry": {...},                # informational (runner handles)
        "timeouts": {...},             # informational
        "resources": [...]
      }
    """
    # Build argv: nexoscli <plugin> <command> --output json --k v ...
    act = payload.get("activity") or "unknown"
    try:
        plugin, command = act.split(".", 1)
    except ValueError:
        plugin, command = act, ""
    argv = ["nexoscli", plugin]
    if command:
        argv.append(command)
    argv.extend(["--output", "json"])
    args = payload.get("args") or {}
    if isinstance(args, dict):
        for k, v in args.items():
            flag = f"--{k.replace('_','-')}"
            if isinstance(v, bool):
                if v: argv.append(flag)
            elif isinstance(v, list):
                for item in v:
                    argv.append(flag); argv.append(str(item))
            else:
                argv.append(flag); argv.append(str(v))
    else:
        # Fallback: pass as JSON string
        argv.extend(["--args-json", json.dumps(args)])

    proc = await asyncio.create_subprocess_exec(
        *argv,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    out, err = await proc.communicate()
    stdout = out.decode("utf-8", errors="ignore")
    stderr = err.decode("utf-8", errors="ignore")
    code = proc.returncode
    # We do not implement idempotency here: your CLI/effects ledger already guarantees it.
    return {
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr[:1000],  # cap for safety
    }

# ---------- Workflow: NexosRunner --------------------------------------------

@workflow.defn
class NexosRunner:
    def __init__(self) -> None:
        self._queue: List[Tuple[str, Dict[str, Any]]] = []
        self._paused = False
        self._stopped = False

    # Signals from drainer
    @workflow.signal
    def activitySchedule(self, body: Dict[str, Any]) -> None:
        self._queue.append(("activity", body))

    @workflow.signal
    def timerSchedule(self, body: Dict[str, Any]) -> None:
        self._queue.append(("timer", body))

    @workflow.signal
    def childStart(self, body: Dict[str, Any]) -> None:
        self._queue.append(("child", body))

    @workflow.signal
    def pause(self) -> None:
        self._paused = True

    @workflow.signal
    def resume(self) -> None:
        self._paused = False

    @workflow.signal
    def stop(self) -> None:
        self._stopped = True

    @workflow.query
    def status(self) -> Dict[str, Any]:
        return {
            "paused": self._paused,
            "stopped": self._stopped,
            "queue_len": len(self._queue),
        }

    @workflow.run
    async def run(self, spec_id: str, run_id: str, memo: Dict[str, Any], search_attributes: Dict[str, str]) -> Dict[str, Any]:
        """
        Simple runner: processes incoming signals sequentially.
        The drainer sends schedule signals in spec order.
        """
        results = []
        while True:
            await workflow.wait_condition(lambda: self._stopped or (not self._paused and len(self._queue) > 0))
            if self._stopped:
                break
            kind, body = self._queue.pop(0)

            if kind == "activity":
                # Map to CLI activity
                timeout = timedelta(minutes=60)
                res = await workflow.execute_activity(
                    run_cli_activity, body, start_to_close_timeout=timeout
                )
                results.append({"step": body.get("step_id"), "kind": "activity", "res": res})

                if int(res.get("exit_code", 1)) != 0:
                    # bubble failure; Temporal will retry workflow or we can continue based on policy (keep minimal here)
                    raise RuntimeError(f"Activity failed: {res.get('stderr','')[:200]}")
            elif kind == "timer":
                dur_ms = int((body.get("duration_ms") or 0))
                await workflow.sleep(timedelta(milliseconds=dur_ms))
                results.append({"step": body.get("step_id"), "kind": "timer"})
            elif kind == "child":
                child_spec = body.get("child_spec")
                child_id = f"{child_spec}:{run_id}-child"
                # Reuse same workflow type; pass minimal args
                handle = await workflow.start_child_workflow(
                    NexosRunner.run, child_spec, run_id, {}, {},
                    id=child_id, task_queue=workflow.task_queue(),
                )
                wait = bool(body.get("wait", False))
                if wait:
                    await handle.result()
                results.append({"step": body.get("step_id"), "kind": "child", "child_id": child_id})

        return {"ok": True, "processed": len(results), "results": results}

# ---------- Worker bootstrap --------------------------------------------------

async def _main_async() -> None:
    target     = os.getenv("TEMPORAL_TARGET", "127.0.0.1:7233")
    namespace  = os.getenv("TEMPORAL_NAMESPACE", "default")
    task_queue = os.getenv("TEMPORAL_TASK_QUEUE", "nexos")
    build_id   = os.getenv("NEXOS_BUILD_ID", "dev-A")  # Worker Build ID

    client = await Client.connect(target, namespace=namespace)
    # Note: Build ID is specified on Worker (versioning sets)
    async with Worker(
        client,
        task_queue=task_queue,
        workflows=[NexosRunner],
        activities=[run_cli_activity],
        build_id=build_id,
    ):
        print(f"[worker] running on {target}/{namespace}, tq={task_queue}, build_id={build_id}", flush=True)
        await asyncio.Event().wait()  # run forever

def main() -> None:
    try:
        asyncio.run(_main_async())
    except KeyboardInterrupt:
        pass

if __name__ == "__main__":
    main()


⸻

3) Outbox drainer (client → server)

temporal_worker/drain_outbox.py

import os, sys, json, asyncio, shutil
from pathlib import Path
from typing import Any, Dict, Tuple

from temporalio.client import Client

OUTBOX = Path(".nexoscli/engine/outbox")
PROCESSED = OUTBOX / "processed"
PROCESSED.mkdir(parents=True, exist_ok=True)

def wf_id_of(env: Dict[str, Any]) -> str:
    # Keep one workflow per run
    return f"{env['workflow']}:{env['run_id']}"

async def _apply(env: Dict[str, Any], client: Client, task_queue: str, use_search_attrs: bool) -> None:
    op = env["op"]
    wf_id = wf_id_of(env)
    attrs = env.get("attrs", {})
    memo  = attrs.get("memo", {}) or {}
    sa    = attrs.get("search_attributes", {}) or {}
    if not use_search_attrs:
        # put SA into memo if Temporal SA fields aren't registered
        memo["_search_attributes_shadow"] = sa
        sa = {}

    if op == "workflow.start":
        # args for NexosRunner.run(spec_id, run_id, memo, search_attrs)
        await client.start_workflow(
            "NexosRunner.run",
            env["workflow"],
            env["run_id"],
            memo,
            sa,
            id=wf_id,
            task_queue=task_queue,
            memo=memo,
            search_attributes=sa,
        )
        return

    # Signals to an existing workflow
    name = env.get("name") or ""
    body = env.get("body") or {}
    if op == "activity.schedule":
        await client.signal_workflow(wf_id, "activitySchedule", body)
    elif op == "timer.schedule":
        await client.signal_workflow(wf_id, "timerSchedule", body)
    elif op == "child.start":
        await client.signal_workflow(wf_id, "childStart", body)
    elif op == "workflow.signal":
        await client.signal_workflow(env["workflow"], name, body)
    else:
        print(f"[drain] unknown op: {op}")

async def _drain_once(dry_run: bool = False) -> int:
    target     = os.getenv("TEMPORAL_TARGET", "127.0.0.1:7233")
    namespace  = os.getenv("TEMPORAL_NAMESPACE", "default")
    task_queue = os.getenv("TEMPORAL_TASK_QUEUE", "nexos")
    use_sa     = os.getenv("TEMPORAL_USE_SA", "0") == "1"

    client = None
    if not dry_run:
        client = await Client.connect(target, namespace=namespace)

    files = sorted([p for p in OUTBOX.glob("*.json") if p.is_file()])
    count = 0
    for p in files:
        env = json.loads(p.read_text(encoding="utf-8"))
        if dry_run:
            print(f"[dry] would apply -> {env['op']} {env['workflow']} run={env['run_id']}")
        else:
            await _apply(env, client, task_queue, use_sa)
        shutil.move(str(p), str(PROCESSED / p.name))
        count += 1
    return count

def main() -> None:
    dry = os.getenv("DRY_RUN", "0") == "1" or "--dry-run" in sys.argv
    try:
        n = asyncio.run(_drain_once(dry_run=dry))
        print(f"[drain] processed={n}, dry={dry}")
    except KeyboardInterrupt:
        pass

if __name__ == "__main__":
    main()


⸻

4) Minimal doc: how to run the smoke

docs/TEMPORAL_BRIDGE.md

# Temporal Bridge (T‑0) — Outbox → Temporal

## Prereqs
- Temporal server / Cloud namespace reachable (or local `temporal server start-dev`)
- Python 3.11/3.12 + `uv` (or `pip`)

## Install
```bash
cd temporal_worker
uv sync         # or: pip install -e .

Start worker

export TEMPORAL_TARGET=127.0.0.1:7233
export TEMPORAL_NAMESPACE=default
export TEMPORAL_TASK_QUEUE=nexos
export NEXOS_BUILD_ID=dev-A
uv run nexos-temporal-worker

Produce outbox from NEXOS

# Ensure a workflow spec exists (e.g., docs/workflows/WF-demo.yaml)
./target/debug/nexoscli workflow run --spec docs/workflows/WF-demo.yaml --engine temporal --output json
ls .nexoscli/engine/outbox

Drain outbox to Temporal

cd temporal_worker
export TEMPORAL_TARGET=127.0.0.1:7233
export TEMPORAL_NAMESPACE=default
uv run nexos-temporal-drain --dry-run   # inspect
uv run nexos-temporal-drain             # apply

Observe
	•	Jaeger / UI: confirm workflow NexosRunner started on task queue nexos
	•	Signals arrive as steps; worker executes:
	•	activity.schedule → activity runs nexoscli ... --output json
	•	timer.schedule → workflow.sleep(...)
	•	child.start → start_child_workflow(...)

Notes
	•	If Search Attributes aren’t registered in the namespace, set TEMPORAL_USE_SA=0 (default). Values are shadowed in memo._search_attributes_shadow.
	•	Build ID is set on the Worker. Later you can adopt Worker Versioning sets.

---

## 5) Optional: tiny Rust test to verify outbox exists (no Temporal dependency)

**`nexoscli-bin/tests/workflow_temporal_smoke.rs`**
```rust
use assert_cmd::prelude::*;
use predicates::str::contains;
use std::{fs, process::Command};

#[test]
fn run_engine_temporal_writes_outbox() {
    let spec = r#"version: "0.1"
id: WF-smoke
run: { build_id: "build-A", change_ids: ["CH_V1"] }
steps:
  - id: a1
    activity: "issue.new"
    args: { title: "smoke" }
  - id: wait
    timer: { duration_ms: 10 }
"#;
    std::fs::create_dir_all("docs/workflows").unwrap();
    std::fs::write("docs/workflows/WF-smoke.yaml", spec).unwrap();

    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","run","--spec","docs/workflows/WF-smoke.yaml","--engine","temporal","--output","json"])
        .assert().success().stdout(contains("\"scheduled\":2"));

    let entries: Vec<_> = fs::read_dir(".nexoscli/engine/outbox").unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .filter(|n| n.contains("WF-smoke"))
        .collect();
    assert!(entries.iter().any(|n| n.contains("workflow-start")));
    assert!(entries.iter().any(|n| n.contains("activity-schedule")));
    assert!(entries.iter().any(|n| n.contains("timer-schedule")));
}


⸻

6) Acceptance criteria (T‑0)
	•	Running nexoscli workflow run --engine temporal does not execute locally and writes outbox envelopes (start + one per step).
	•	Running uv run nexos-temporal-drain sends StartWorkflow then Signals to the correct workflow ID.
	•	The worker (uv run nexos-temporal-worker) consumes those signals:
	•	executes at least one activity by shelling out to nexoscli,
	•	executes at least one timer,
	•	can start a child workflow when signaled.
	•	No changes to your Rust plugin contracts. The same YAML specs work in both engines.
	•	Determinism (T‑2) still in force: build_id / change_ids present in envelopes and preserved by the worker.

⸻

Why this completes the “flip switch”

You now have:
	•	Spec ↔ plan/run surfaces unchanged.
	•	Local engine for fast development.
	•	Temporal engine that produces outbox.
	•	A drainer that translates outbox → Temporal API.
	•	A worker that runs the same activities/timers/children with a Build ID.

When you’re ready, we can (add to ROADMAP_NEXT.md for future suggestion):
	•	add heartbeats to run_cli_activity,
	•	move search attributes from memo shadow into real SAs (once registered),
	•	add visibility queries to the Python client for a tiny status CLI,
	•	introduce Worker Versioning sets and Build ID routing.