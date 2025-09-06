---
schema: readme.v0
title: "project/current"
summary: "The single active plan in progress; managed exclusively by the CLI."
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
  - "The one plan that is currently being implemented"
  - "Rules and gates to prevent multiple concurrent plans"
out_of_scope:
  - "Manual file moves or front-matter edits"
inputs:
  - "PLAN-<id>_<slug>.md — exactly one plan file"
outputs:
  - "Plan moved to done/ on finalize; task and bundle artifacts"
tags: ["plans","current"]
auto:
  children_glob: []
  dir_tree_depth: 0
---

# Current

There must be exactly one plan in progress. If more than one is detected, halt and resolve to avoid drift and accidental complexity.

## Purpose

Provide a single source of truth for the plan that is currently being implemented.

## In‑scope

- Enforcement that only one plan exists in this folder
- Running status and lifecycle operations via CLI

## Out‑of‑scope

- Manual movement of plans
- Editing CLI-managed front‑matter fields

## Workflows

- Inspect current plan:
  ```
  nexoscli dev plans list --where current --output json
  nexoscli dev plans status --plan docs/project/current/PLAN-<id>_<slug>.md --output json
  ```
- Finalize the current plan (records implemented_in, ends task + bundle, moves current → done):
  ```
  nexoscli dev plans finalize --output json
  ```

## Rules

- Exactly one `PLAN-<id>_<slug>.md` must exist under `current/`.
- The plan’s front‑matter fields are maintained by the CLI:
  `status, plan_id, slug, task_id, started_at, completed_at, implemented_in, source`.
- Do not manually move plan files.

## Checks & Gates

- Structure and readme policy:
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- CI gate for repo policy:
  ```
  nexoscli dev doctor --ci --checks repo --output json
  ```

## Interfaces

- Inputs: the single active plan file
- Outputs: completed plan moved to `done/` with recorded metadata

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
- [PLAN-49_readme-restructure.md](./PLAN-49_readme-restructure.md)
<!-- END: AUTO -->
