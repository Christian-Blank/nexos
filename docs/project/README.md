# Project Plans — Lifecycle and Structure

This folder contains the plan backlog and lifecycle artifacts. Plans are managed exclusively via the CLI to keep behavior deterministic and observable.

## Directory structure

- docs/project/next/ — candidate plans not yet started
- docs/project/current/ — the single plan in progress
- docs/project/done/ — completed plans (immutable history)
- docs/project/archived/ — retired/abandoned plans (optional)

## File naming

Plans are markdown files named with an ID and a slug:

- PLAN-<id>_<slug>.md (preferred)
- plan-<id>_<slug>.md (legacy accepted)

Examples:
- PLAN-36_export-helpers-csv.md
- plan-33_events-schema-and-normalizer.md

## Manage plans with nexoscli

Do not hand-edit front-matter or move files manually. Use the dev plans … commands:

- Start the next plan (creates + starts a task, updates front‑matter, moves next → current):
  nexoscli dev plans start --output json

- Finalize the current plan (records implemented_in, ends task + bundle, moves current → done):
  nexoscli dev plans finalize --output json

- List plans:
  nexoscli dev plans list --where next   --output json
  nexoscli dev plans list --where current --output json
  nexoscli dev plans list --where done    --output json

- Status for a specific plan file:
  nexoscli dev plans status --plan docs/project/done/PLAN-34_plan-and-dev-cli.md --output json

## Front‑matter fields (maintained by CLI)

The CLI manages these fields inside each plan’s YAML front‑matter:

- status: in-progress | done
- plan_id: string (derived from filename)
- slug: string (derived from filename)
- task_id: string (on start)
- started_at: RFC3339 timestamp (on start)
- completed_at: RFC3339 timestamp (on finalize)
- implemented_in: git SHA (on finalize; defaults to HEAD)
- source: string (e.g., "next" when started from backlog)

## Troubleshooting

- Error: task 'TASK-…' is active; pause it before starting/resuming another task
  - Pause the current active task:
    nexoscli task pause
  - Retry:
    nexoscli dev plans start --output json
  - If you must end the previous task first:
    nexoscli task end --task <TASK_ID> || true
    nexoscli dev plans start --output json

- Multiple plans under current/
  - By policy, only one plan should be in current/. If there are multiple, pass an explicit path:
    nexoscli dev plans finalize --plan docs/project/current/PLAN-XX_slug.md --output json

## Notes

- Prefer generators and CLI flows; avoid ad‑hoc scripts and inline Python.
- All commands emit ctx.ok/ctx.err JSON envelopes for observability and CI consumption.
