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

## Interfaces

- Inputs: candidate plan files
- Outputs: started plan moved to `current/`

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
_No plan files found in this folder._
<!-- END: AUTO -->
