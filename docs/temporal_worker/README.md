# nexos-temporal-bridge

Python bridge used by Nexos to consume Temporal Outbox envelopes written by the workflow plugin and forward them to a Temporal server.

Components:
- worker.py: Hosts a Temporal workflow NexosRunner and activity run_cli_activity
- drain_outbox.py: Drains .nexoscli/engine/outbox/*.json and maps to Temporal start/signals

Install/Run (uv recommended):
- Run worker:
  uv run --project docs/temporal_worker nexos-temporal-worker
- Drain outbox:
  uv run --project docs/temporal_worker nexos-temporal-drain [--dry-run]

Environment:
- TEMPORAL_ADDRESS (default 127.0.0.1:7233)
- TEMPORAL_NAMESPACE (default default)
- TEMPORAL_TASK_QUEUE (default nexos)
- TEMPORAL_USE_SA (default 1, set 0 to drop search_attributes)
- NEXOSCLI_BIN (optional path to the nexoscli binary)

See docs/TEMPORAL_BRIDGE.md for the full runbook.
