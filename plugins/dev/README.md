# dev plugin — Developer ergonomics and Plan Lifecycle

The dev plugin provides repository health tools, documentation generators, telemetry reports, and a first‑class Plan Lifecycle CLI. All commands emit ctx.ok/ctx.err JSON envelopes when `--output json` is used.

## Summary

- Initialize dev environment (idempotent): `nexoscli dev init`
- Repo checks: `nexoscli dev plugins check`
- Generate docs: `nexoscli dev docs build`
- Telemetry report: `nexoscli dev telemetry report`
- Plan Lifecycle:
  - `nexoscli dev plans list|next|status`
  - `nexoscli dev plans start` (creates+starts task, updates front‑matter, moves next→current)
  - `nexoscli dev plans finalize` (records implemented_in, ends task+bundle, moves current→done)

## Commands

### dev init
Initialize `.nexoscli/` with a manifest, logs directory, and optional config.

- Plan (dry-run):
  nexoscli dev init --dry-run --output json

- Apply (idempotent):
  nexoscli dev init --yes --output json

### dev plugins check
Verify folder structure and registration of plugins/*; optionally fix missing READMEs.

- Check:
  nexoscli dev plugins check --output json

- Auto-create missing READMEs:
  nexoscli dev plugins check --fix-readmes --output json

### dev docs build
Generate CLI reference and export machine tool specs summary into `docs/CLI_REFERENCE.md` and `docs/RULES.md` (rules human export also run separately).

- Build:
  nexoscli dev docs build --output json

### dev telemetry report
Aggregate events by (plugin,command) with counts, success/fail, durations, and error leaderboard.

- Last 24h (default):
  nexoscli dev telemetry report --since 24h --output json

## Plan Lifecycle CLI

All plan operations are CLI-native and observable. Do not hand-edit front‑matter YAML or move plan files manually.

Directory structure:
- docs/project/next — candidate plans (PLAN-<id>_<slug>.md)
- docs/project/current — the single active plan
- docs/project/done — completed plans
- docs/project/archived — retired plans

### dev plans list
List plans across the project plan folders.

- Examples:
  nexoscli dev plans list --where next --output json
  nexoscli dev plans list --where current --output json
  nexoscli dev plans list --where done --output json

### dev plans next
Show the next candidate (first under docs/project/next).

- Example:
  nexoscli dev plans next --output json

### dev plans status
Inspect a specific plan’s front‑matter and derived ids (plan_id, slug).

- Example:
  nexoscli dev plans status --plan docs/project/done/PLAN-34_plan-and-dev-cli.md --output json

### dev plans start
Start a plan: creates+starts a task, updates front‑matter, moves next → current.

- Default (pick first in next/):
  nexoscli dev plans start --output json

- Dry-run (no write/move):
  nexoscli dev plans start --dry-run --output json

Fields written to front‑matter:
- status=in-progress
- plan_id, slug (derived from filename)
- task_id
- started_at (RFC3339)
- source="next"

### dev plans finalize
Finalize a plan: record `implemented_in` (HEAD by default), end task + bundle, write completed_at + status=done, move current → done.

- Default (only plan under current/):
  nexoscli dev plans finalize --output json

- Override commit/BUNDLE:
  nexoscli dev plans finalize --implemented-in <SHA> --bundle-out ./my-bundle.tgz --output json

### Troubleshooting
- Active task prevents starting/resuming:
  Error: task 'TASK-…' is active; pause it before starting/resuming another task

  Resolution:
  - Pause current task:
    nexoscli task pause
  - Or end explicitly (best-effort):
    nexoscli task end --task <TASK_ID> || true
  - Retry:
    nexoscli dev plans start --output json

### Notes
- Prefer `--output json` for machine flows; parse `data` and `meta.context` fields.
- All operations emit envelopes via ctx.ok/ctx.err so runs are auditable.
- See `docs/project/README.md` for structure and guidance; prompts in `docs/prompts/cline/` for start/finalize workflows.
