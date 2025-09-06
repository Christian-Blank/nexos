---
schema: readme.v0
title: "project/paused"
summary: "Temporarily paused plans; resume to current/ after sanity checks."
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
  - "Temporarily parked plans while other work completes"
  - "Resume flow with repository/plan state reconciliation"
out_of_scope:
  - "Direct movement to next/ (must resume to current/ first)"
inputs:
  - "PLAN-<id>_<slug>.md files temporarily moved here"
outputs:
  - "Plan resumed to current/ or returned to backlog for reprioritization"
tags: ["plans","paused"]
auto:
  children_glob: []
  dir_tree_depth: 0
---

# Paused

Plans temporarily paused. When resuming, first reconcile plan intent with the current repository state to avoid drift.

## Purpose

Provide a clear holding area for in‑flight work that must temporarily stop while higher priority work completes.

## In‑scope

- Pausing a single current plan to resolve higher‑priority work
- Resuming the paused plan with sanity checks

## Out‑of‑scope

- Moving paused plans directly to `next/`
- Skipping reconciliation before resuming

## Workflows

- Pause current plan (policy / manual step):
  - Move the plan file from `current/` to `paused/` with a short rationale in the plan header.
- Resume paused plan:
  1) Analyze and sanity check the paused plan and the repository state:
     - Compare plan intent and changes already made with current repo to spot gaps and drift.
  2) Move back to `current/`.
  3) Continue implementation via CLI workflows.
- Requeue to backlog:
  - If not ready to resume, move to `backlog/` for reprioritization and potential renumbering.

## Rules

- Do not move directly from `paused/` to `next/`.
- Before resuming to `current/`, perform a reconciliation pass to detect drift (files moved, conflicts, or outdated assumptions).
- Maintain one active plan at a time in `current/`.

## Checks & Gates

- Repo policy/readmes (informational here):
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- CI repo gate:
  ```
  nexoscli dev doctor --ci --checks repo --output json
  ```

## Interfaces

- Inputs: paused plan file(s)
- Outputs: resumed plan returned to `current/`, or requeued to `backlog/`

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
_No plan files currently listed in this folder._
<!-- END: AUTO -->
