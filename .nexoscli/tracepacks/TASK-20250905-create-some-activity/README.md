# Task TracePack

This bundle captures a single task's history:
- `task/task.json` — task ledger (id, stage, sessions)
- `events.ndjson`  — all events tagged with this task_id
- `patches/`       — git diffs captured via `nexoscli term run`
- `status/`        — `git status --porcelain` snapshots
- `cline/tasks/`   — normalized Cline tasks in the time window (+/- 5m)

Use `nexoscli replay plan/run` for patch-level replay on a safe branch.
