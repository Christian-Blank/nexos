from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import shutil
from pathlib import Path
from typing import Any, Dict, List, Tuple

from temporalio.client import Client

# Default locations relative to repository root
DEFAULT_OUTBOX = Path(".nexoscli/engine/outbox")


def _env(name: str, default: str) -> str:
    return os.getenv(name, default)


def _load_envelopes(outbox: Path) -> List[Tuple[Path, Dict[str, Any]]]:
    files = [p for p in outbox.glob("*.json") if p.is_file()]
    out: List[Tuple[Path, Dict[str, Any]]] = []
    for p in files:
        try:
            v = json.loads(p.read_text())
            out.append((p, v))
        except Exception:
            # Skip unreadable or malformed files
            continue
    # Sort by embedded timestamp (ts), fallback to filename for stability
    out.sort(key=lambda t: t[1].get("ts", t[0].name))
    return out


async def _ensure_workflow_started(
    client: Client,
    wf_id: str,
    attrs: Dict[str, Any],
    body: Dict[str, Any],
    task_queue: str,
    dry: bool,
) -> None:
    if dry:
        return
    # Import workflow class from sibling module in this project
    from worker import NexosRunner  # type: ignore

    # Build memo including determinism fields and run_id for traceability
    memo = dict(attrs.get("memo", {}))
    memo.update(
        {
            "nexos_run_id": attrs.get("run_id") or "",
            "build_id": attrs.get("build_id"),
            "change_ids": attrs.get("change_ids"),
            "spec_sha256": attrs.get("spec_sha256"),
        }
    )

    # Optionally pass search attributes (some clusters require registration)
    use_sa = os.getenv("TEMPORAL_USE_SA", "1").lower() not in ("0", "false")
    sa = attrs.get("search_attributes") if use_sa else None
    if not isinstance(sa, dict):
        sa = None

    try:
        await client.start_workflow(
            NexosRunner.run,
            id=wf_id,
            task_queue=task_queue,
            memo=memo,
            search_attributes=sa,
        )
    except Exception as e:
        # If workflow already exists/running, consider start as satisfied
        msg = str(e)
        if "already started" in msg or "ALREADY_EXISTS" in msg:
            return
        # For other errors, re-raise to leave file unprocessed for retry
        raise


async def _signal(
    client: Client, wf_id: str, name: str, payload: Any, dry: bool
) -> None:
    if dry:
        return
    if payload is None or payload == {}:
        await client.signal_workflow(wf_id, name)
    else:
        await client.signal_workflow(wf_id, name, payload)


async def drain(outbox_dir: Path, dry: bool = False) -> None:
    addr = _env("TEMPORAL_ADDRESS", "127.0.0.1:7233")
    namespace = _env("TEMPORAL_NAMESPACE", "default")
    task_queue = _env("TEMPORAL_TASK_QUEUE", "nexos")

    client = await Client.connect(addr, namespace=namespace)

    processed_dir = outbox_dir / "processed"
    processed_dir.mkdir(parents=True, exist_ok=True)

    for path, v in _load_envelopes(outbox_dir):
        op = v.get("op")
        wf = v.get("workflow")
        attrs = v.get("attrs", {}) or {}
        body = v.get("body", {}) or {}
        # Attach run_id into attrs for memo propagation in _ensure_workflow_started
        attrs["run_id"] = v.get("run_id")

        try:
            if op in ("start", "workflow.start"):
                await _ensure_workflow_started(client, wf, attrs, body, task_queue, dry)
            elif op == "activity.schedule":
                await _signal(client, wf, "activitySchedule", body, dry)
            elif op == "timer.schedule":
                await _signal(client, wf, "timerSchedule", body, dry)
            elif op == "child.start":
                await _signal(client, wf, "childStart", body, dry)
            elif op in ("signal", "workflow.signal"):
                name = v.get("name")
                await _signal(client, wf, name, body, dry)
            else:
                # Unknown op → skip but still mark processed to avoid tight loops
                pass

            # Move processed file
            shutil.move(str(path), str(processed_dir / path.name))
        except Exception as e:
            # Leave file in place for retry; log to stderr
            print(f"error processing {path.name}: {e}", file=sys.stderr)  # type: ignore[name-defined]
            # Continue with next envelope
            continue


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Drain .nexoscli/engine/outbox envelopes to Temporal."
    )
    ap.add_argument(
        "--outbox",
        type=str,
        default=str(DEFAULT_OUTBOX),
        help="Outbox directory (default: .nexoscli/engine/outbox)",
    )
    ap.add_argument("--dry-run", action="store_true", help="Do not call Temporal APIs")
    args = ap.parse_args()

    outbox_dir = Path(args.outbox)
    outbox_dir.mkdir(parents=True, exist_ok=True)

    asyncio.run(drain(outbox_dir=outbox_dir, dry=args.dry_run))


if __name__ == "__main__":
    main()
