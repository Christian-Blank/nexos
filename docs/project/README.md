---
schema: readme.v0
title: "Project Plans"
summary: "Deterministic plan lifecycle folders (next/current/done/archived/backlog/paused/dumped) managed via the CLI."
kind: domain
owner: "@docs/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-07T00:00:00Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./"
in_scope:
  - "Plan lifecycle documentation and structure"
  - "CLI-native workflows for starting and finalizing plans"
  - "Rules for naming and placement of plans"
out_of_scope:
  - "Manual plan file moves and front-matter edits (must use CLI)"
inputs:
  - "Plan markdown files under lifecycle folders"
outputs:
  - "Updated plan files via CLI; plan indexes"
tags: ["plans","docs"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---

# Project Plans — Lifecycle and Structure

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated children list of plan lifecycle subfolders (next/current/done/archived/backlog/paused/dumped)._
<!-- END: AUTO -->

## Purpose

Provide a deterministic, observable structure for project plans, with CLI-native workflows that enforce consistent front‑matter, movement, and status transitions.

## In‑scope

- Plan lifecycle folders and their constraints
- CLI workflows to start/finalize a plan
- Index blocks (per-folder) to list contained plans

## Out‑of‑scope

- Manual file moves or direct front‑matter mutations
- Non-plan documents (keep those in other docs areas)

## Workflows

- Start the next plan (creates + starts a task, updates front‑matter, moves next → current):
  ```
  nexoscli dev plans start --output json
  ```

- Finalize the current plan (records implemented_in, ends task + bundle, moves current → done):
  ```
  nexoscli dev plans finalize --output json
  ```

- List plans:
  ```
  nexoscli dev plans list --where next    --output json
  nexoscli dev plans list --where current --output json
  nexoscli dev plans list --where done    --output json
  ```

- Status for a specific plan file:
  ```
  nexoscli dev plans status --plan docs/project/done/PLAN-34_plan-and-dev-cli.md --output json
  ```

## Rules

- File naming patterns:
  - Preferred: `PLAN-<id>_<slug>.md`
  - Legacy accepted: `plan-<id>_<slug>.md`
- Single plan under `current/` (policy); all planning transitions via CLI.
- Plan front‑matter fields are maintained by the CLI only:
  - `status`, `plan_id`, `slug`, `task_id`, `started_at`, `completed_at`, `implemented_in`, `source`
- Indexes in subfolder READMEs (id=plans-index) should list only plan files.
- Keep diffs minimal and deterministic; no manual content inside AUTO blocks.

## Checks & Gates

- Repo structure and README guardrails:
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- CI gate for repo policy:
  ```
  nexoscli dev doctor --ci --checks repo --output json
  ```

## Interfaces

- Inputs: plan files in lifecycle folders
- Outputs: updated plan metadata, deterministic indexes in subfolder READMEs

## Additional Notes

This folder contains the plan backlog and lifecycle artifacts. Plans are managed exclusively via the CLI to keep behavior deterministic and observable.

### Directory structure

- docs/project/next/ — candidate plans not yet started
- docs/project/current/ — the single plan in progress
- docs/project/done/ — completed plans (immutable history)
- docs/project/archived/ — retired/abandoned plans (optional)

### File naming (examples)

- `PLAN-36_export-helpers-csv.md`
- `plan-33_events-schema-and-normalizer.md`
