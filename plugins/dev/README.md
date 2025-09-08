---
schema: readme.v0
title: "dev plugin"
summary: "Repo health checks, docs build, telemetry, and CLI-native plan lifecycle."
kind: leaf
owner: "@plugins/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md","../../README.md"]
children: []
source_of_truth:
  - "./src/lib.rs"
  - "./README.md"
in_scope:
  - "Repository health checks and enforcement"
  - "Docs build and machine tool spec exports"
  - "Telemetry aggregation"
  - "Plan lifecycle: start/finalize via CLI"
out_of_scope:
  - "Business/domain plugin functionality (see other plugins)"
inputs:
  - "Workspace configuration and event logs"
outputs:
  - "Docs, reports, and ctx.ok/ctx.err envelopes"
tags: ["plugin","dev"]
auto: { }
---
# dev plugin — Developer ergonomics and Plan Lifecycle

The dev plugin provides repository health tools, documentation generators, telemetry reports, and a first‑class Plan Lifecycle CLI. All commands emit ctx.ok/ctx.err JSON envelopes when `--output json` is used.

## Summary

- Initialize dev environment (idempotent): `nexoscli dev init`
- Repo checks: `nexoscli dev plugins check`
- Generate docs: `nexoscli dev docs build`
- Telemetry report: `nexoscli dev telemetry report`
- Plans verify: `nexoscli dev plans verify`
- Plans index build: `nexoscli dev plans index build` (use `--dry-run` to preview; idempotent)
- Doctor plans check: `nexoscli dev doctor --ci --checks plans`
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

## Plan Lifecycle Hygiene (Plan 50)

- Verify lifecycle invariants:
  ```
  nexoscli dev plans verify --output json
  ```
- Build deterministic README indexes (AUTO blocks id=plans-index; relative links; idempotent):
  ```
  nexoscli dev plans index build --output json
  ```
  Use `--dry-run` to preview changes without writing.
- CI integration (explicit required gate):
  ```
  nexoscli dev doctor --ci --checks plans --output json
  ```

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

### dev plans pause
Pause the current plan (moves docs/project/current → docs/project/paused). Optionally record a rationale note in front‑matter.

- Pause singleton in current/:
  ```
  nexoscli dev plans pause --output json
  ```
- Pause explicit path with rationale:
  ```
  nexoscli dev plans pause --plan docs/project/current/PLAN-610_example.md --reason "blocked by upstream" --output json
  ```
- Dry‑run (no writes/moves):
  ```
  nexoscli dev plans pause --dry-run --output json
  ```

### dev plans resume
Resume a paused plan (moves docs/project/paused → docs/project/current). Enforces that current/ is empty, and runs a reconciliation summary using repo and plans checks.

- Resume singleton in paused/:
  ```
  nexoscli dev plans resume --output json
  ```
- Resume explicit path:
  ```
  nexoscli dev plans resume --plan docs/project/paused/PLAN-611_example.md --output json
  ```
- Dry‑run:
  ```
  nexoscli dev plans resume --dry-run --output json
  ```

### dev plans archive
Archive a plan to docs/project/archived (no other mutations except optional rationale note).

- Archive with reason:
  ```
  nexoscli dev plans archive --plan docs/project/next/PLAN-620_example.md --reason "obsolete" --output json
  ```
- Dry‑run:
  ```
  nexoscli dev plans archive --plan docs/project/next/PLAN-620_example.md --dry-run --output json
  ```

### dev plans dump
Dump a plan to docs/project/dumped (no other mutations except optional rationale note).

- Dump with reason:
  ```
  nexoscli dev plans dump --plan docs/project/next/PLAN-621_example.md --reason "invalid" --output json
  ```
- Dry‑run:
  ```
  nexoscli dev plans dump --plan docs/project/next/PLAN-621_example.md --dry-run --output json
  ```

### dev plans requeue
Requeue a paused plan to docs/project/backlog for reprioritization. Warns on numbered filenames.

- Requeue singleton in paused/:
  ```
  nexoscli dev plans requeue --output json
  ```
- Requeue explicit path:
  ```
  nexoscli dev plans requeue --plan docs/project/paused/PLAN-622_example.md --output json
  ```
- Dry‑run:
  ```
  nexoscli dev plans requeue --dry-run --output json
  ```

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

## Commands

<!-- BEGIN: AUTO (id=commands) -->
- dev docs build — Generate CLI reference and Rules doc
- dev docs commands — Generate CLI reference/catalog and update plugin READMEs
- dev doctor — Environment & hygiene checks
- dev init — Initialize .nexoscli/ (Plan/Apply, idempotent)
- dev log rotate — Rotate .nexoscli/log/events.jsonl into timestamped segments with chain manifests
- dev log status — Show rotation/index status
- dev plans archive — Archive a plan: move to archived/
- dev plans dump — Dump a plan: move to dumped/
- dev plans finalize — Finalize a plan: mark done, end task, bundle, move current→done
- dev plans index build — Refresh AUTO (id=plans-index) blocks in docs/project/* READMEs
- dev plans list — List plans
- dev plans next — Show the next plan candidate under docs/project/next
- dev plans pause — Pause current plan: move current→paused
- dev plans requeue — Requeue a paused plan to backlog/
- dev plans resume — Resume a paused plan: move paused→current
- dev plans start — Start a plan: create+start task, update front-matter, move next→current
- dev plans status — Show plan front-matter and derived ids
- dev plans verify — Validate plan lifecycle folder rules
- dev plugin new — Scaffold a new plugin under plugins/<name>
- dev plugins check — Verify structure and registration for all plugins/*
- dev repo args-enforce — Enforce dual-derive and ban ArgMatches parsing (scoped)
- dev repo check — Validate structure against policy
- dev repo policy init — Write default .nexoscli/repo_policy.yaml
- dev repo readmes — Scaffold missing folder READMEs
- dev schemas export — Write JSON Schemas to a directory
- dev telemetry hai — Aggregate AI-interaction metrics from AIM streams (.nexoscli/agents/streams/*)
- dev telemetry report — Aggregate events.jsonl → counts by command (success/fail)
<!-- END: AUTO -->
