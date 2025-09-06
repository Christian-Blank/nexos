from __future__ import annotations

import asyncio
import json
import os
import sys
from datetime import timedelta
from typing import Any, Dict, List, Optional

from temporalio import activity, workflow
from temporalio.client import Client
from temporalio.worker import Worker


# ------------------ Activity ------------------


@activity.defn
async def run_cli_activity(activity_name: str, args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Execute "nexoscli <plugin> <command...> --output json ..." with arg mapping:
      - bool true  => --k
      - str/num    => --k v
      - array      => repeated --k item
      - object     => --k "<json>"
    """
    def parse_activity(act: str) -> List[str]:
        parts = act.split(".")
        if len(parts) < 2:
            raise ValueError(f"invalid activity '{act}', want plugin.command")
        return [parts[0], *parts[1:]]

    argv: List[str] = ["nexoscli", *parse_activity(activity_name), "--output", "json"]

    if isinstance(args, dict):
        for k, v in args.items():
            flag = f"--{k}"
            if isinstance(v, bool):
                if v:
                    argv.append(flag)
            elif isinstance(v, (str, int, float)):
                argv += [flag, str(v)]
            elif isinstance(v, list):
                for item in v:
                    if isinstance(item, (str, int, float)):
                        argv += [flag, str(item)]
                    else:
                        argv += [flag, json.dumps(item)]
            else:
                argv += [flag, json.dumps(v)]

    # Allow overriding nexoscli path (default assumes it is on PATH)
    nexos_bin = os.getenv("NEXOSCLI_BIN")
    if nexos_bin:
        argv[0] = nexos_bin

    proc = await asyncio.create_subprocess_exec(
        *argv,
        stdout=asyncio.subprocess.PIPE,
        stderr=sys.stderr,
    )
    out, _ = await proc.communicate()
    txt = out.decode("utf-8", errors="replace")
    try:
        return json.loads(txt)
    except json.JSONDecodeError:
        return {"stdout": txt, "exit_code": proc.returncode}


# ------------------ Workflow ------------------


@workflow.defn
class NexosRunner:
    """
    Minimal bridge workflow:
      - Maintains an in-memory FIFO queue of scheduled items (activity/timer/child)
      - Signals enqueue items; run loop processes them in order
      - Control signals: pause/resume/stop
      - Query: status (paused/stopped/processed/last_event)
    """

    def __init__(self) -> None:
        self.workflow_id: str = workflow.info().workflow_id
        self.paused: bool = False
        self.stopped: bool = False
        self.queue: List[Dict[str, Any]] = []
        self.last_event: Optional[str] = None
        self.processed: int = 0

    @workflow.run
    async def run(self) -> str:
        while not self.stopped:
            # Wait for work or state change
            await workflow.wait_condition(lambda: self.stopped or self.paused or len(self.queue) > 0)

            # Handle pause
            if self.stopped:
                break
            if self.paused:
                # Block until resume or stop
                await workflow.wait_condition(lambda: self.stopped or not self.paused)
                if self.stopped:
                    break

            # Pop next item if any
            if not self.queue:
                # Spurious wakeup (pause/resume), continue loop
                continue

            item = self.queue.pop(0)
            kind = item.get("op")
            body = item.get("body", {})

            if kind == "activity.schedule":
                timeouts = body.get("timeouts", {})
                stc_ms = timeouts.get("start_to_close_ms")
                # Execute activity (retry policy not mapped in minimal bridge)
                await workflow.execute_activity(
                    run_cli_activity,
                    body.get("activity", ""),
                    body.get("args", {}),
                    start_to_close_timeout=timedelta(milliseconds=stc_ms) if stc_ms else timedelta(minutes=15),
                )
                self.last_event = f"activity:{body.get('step_id')}"
                self.processed += 1

            elif kind == "timer.schedule":
                dur_ms = int(body.get("duration_ms", 0))
                # workflow.sleep expects seconds (float ok)
                await workflow.sleep(dur_ms / 1000.0)
                self.last_event = f"timer:{body.get('step_id')}"
                self.processed += 1

            elif kind == "child.start":
                # Fire a child NexosRunner; minimal mapping
                step_id = body.get("step_id", "child")
                child_id = f"{self.workflow_id}-child-{step_id}"
                wait = bool(body.get("wait", False))
                handle = await workflow.start_child_workflow(
                    NexosRunner.run,
                    id=child_id,
                )
                if wait:
                    await handle.result()
                self.last_event = f"child:{step_id}"
                self.processed += 1

            elif kind == "signal":
                # Pass-through named control signals (pause/resume/stop)
                name = item.get("name")
                if name == "pause":
                    self.paused = True
                elif name == "resume":
                    self.paused = False
                elif name == "stop":
                    self.stopped = True
                self.last_event = f"signal:{name}"

            else:
                # Unknown: ignore
                self.last_event = f"unknown:{kind}"

        return "stopped" if self.stopped else "completed"

    # Schedule signals (enqueue items)
    @workflow.signal
    async def activitySchedule(self, body: Dict[str, Any]) -> None:
        self.queue.append({"op": "activity.schedule", "body": body})

    @workflow.signal
    async def timerSchedule(self, body: Dict[str, Any]) -> None:
        self.queue.append({"op": "timer.schedule", "body": body})

    @workflow.signal
    async def childStart(self, body: Dict[str, Any]) -> None:
        self.queue.append({"op": "child.start", "body": body})

    # Control signals
    @workflow.signal
    async def pause(self) -> None:
        self.paused = True

    @workflow.signal
    async def resume(self) -> None:
        self.paused = False

    @workflow.signal
    async def stop(self) -> None:
        self.stopped = True

    # Query
    @workflow.query
    def status(self) -> Dict[str, Any]:
        return {
            "paused": self.paused,
            "stopped": self.stopped,
            "processed": self.processed,
            "last_event": self.last_event,
        }


# ------------------ Worker main ------------------


async def _run_worker() -> None:
    addr = os.getenv("TEMPORAL_ADDRESS", "127.0.0.1:7233")
    namespace = os.getenv("TEMPORAL_NAMESPACE", "default")
    task_queue = os.getenv("TEMPORAL_TASK_QUEUE", "nexos")

    client = await Client.connect(addr, namespace=namespace)

    # NEXOS_BUILD_ID can be used for advanced versioning; omitted in minimal bridge
    _ = os.getenv("NEXOS_BUILD_ID")

    worker = Worker(
        client,
        task_queue=task_queue,
        workflows=[NexosRunner],
        activities=[run_cli_activity],
    )
    await worker.run()


def main() -> None:
    asyncio.run(_run_worker())


if __name__ == "__main__":
    main()
