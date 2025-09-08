---
schema: readme.v0
title: "project/next"
summary: "Candidate plans not yet started. Picked in ascending plan_id order via CLI."
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
  - "Staging area for candidate plans before implementation"
  - "Naming and ordering rules for upcoming work"
out_of_scope:
  - "Manual movement of plans (use CLI)"
inputs:
  - "PLAN-<id>_<slug>.md files staged for start"
outputs:
  - "First plan moved to current/ by CLI"
tags: ["plans","next"]
auto:
  children_glob: []
  dir_tree_depth: 0
---

# Next

Candidate plans ready for implementation. Plans are selected in ascending plan_id order and moved to `current/` using the CLI.

## Purpose

Provide a deterministic queue of plans to start next, respecting dependencies and ordering.

## In‑scope

- Queueing of candidate plans
- Enforcing naming and ordering rules

## Out‑of‑scope

- Manual reordering without updating plan numbers
- Manual file moves or front‑matter edits

## Workflows

- Show next candidate:
  ```
  nexoscli dev plans next --output json
  ```
- Start next plan (creates+starts task; moves next → current):
  ```
  nexoscli dev plans start --output json
  ```

## Rules

- File naming:
  - Preferred: `PLAN-<id>_<slug>.md`
  - Legacy accepted: `plan-<id>_<slug>.md`
- Ordering must reflect build sequence. If order changes, update plan_id accordingly and ensure dependencies are respected.
- Do not manually move files; use `dev plans start`.

## Checks & Gates

- Repo policy and readmes:
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- CI repo gate:
  ```
  nexoscli dev doctor --ci --checks repo --output json
  ```
- Plan lifecycle hygiene (Plan 50):
  ```
  nexoscli dev plans verify --output json
  nexoscli dev plans index build --output json
  nexoscli dev doctor --ci --checks plans --output json
  ```

## Interfaces

- Inputs: candidate plan files
- Outputs: started plan moved to `current/`

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
- [PLAN-58_repo-structure-gates-and-enforcement.md](./PLAN-58_repo-structure-gates-and-enforcement.md) (depends_on: [55])
- [PLAN-59_migrate-demo-plugin.md](./PLAN-59_migrate-demo-plugin.md) (depends_on: [58])
- [PLAN-60_migrate-graph-plugin.md](./PLAN-60_migrate-graph-plugin.md) (depends_on: [58])
- [PLAN-61_migrate-rules-plugin.md](./PLAN-61_migrate-rules-plugin.md) (depends_on: [58])
- [PLAN-62_migrate-term-plugin.md](./PLAN-62_migrate-term-plugin.md) (depends_on: [58])
- [PLAN-63_migrate-issue-plugin.md](./PLAN-63_migrate-issue-plugin.md) (depends_on: [58])
- [PLAN-64_migrate-cline-plugin.md](./PLAN-64_migrate-cline-plugin.md) (depends_on: [58])
- [PLAN-65_migrate-workflow-plugin.md](./PLAN-65_migrate-workflow-plugin.md) (depends_on: [58])
- [PLAN-66_migrate-replay-plugin.md](./PLAN-66_migrate-replay-plugin.md) (depends_on: [58])
- [PLAN-67_migrate-task-plugin.md](./PLAN-67_migrate-task-plugin.md) (depends_on: [58])
- [PLAN-68_migrate-bundle-plugin.md](./PLAN-68_migrate-bundle-plugin.md) (depends_on: [58])
- [PLAN-69_dev-plugin-completion-and-cleanup.md](./PLAN-69_dev-plugin-completion-and-cleanup.md) (depends_on: [58])
- [PLAN-70_repo-wide-hardening-and-finalize.md](./PLAN-70_repo-wide-hardening-and-finalize.md) (depends_on: [58,59,60,61,62,63,64,65,66,67,68,69])
<!-- END: AUTO -->
