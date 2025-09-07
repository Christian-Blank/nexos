---
schema: readme.v0
title: "project/backlog"
summary: "Unnumbered ideas and candidate plans awaiting prioritization; move to next/ once numbered."
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
  - "Collect and refine ideas/tasks before numbering"
  - "Curation and pruning to keep backlog actionable"
out_of_scope:
  - "Numbered plans (those belong in next/)"
  - "Manual plan movements without review"
inputs:
  - "Unnumbered markdown documents capturing plan intent"
outputs:
  - "Prioritized and numbered plans moved to next/"
tags: ["plans","backlog"]
auto:
  children_glob: []
  dir_tree_depth: 0
---

# Backlog

Ideas and candidate plans not yet numbered. When ready, assign a plan_id and move to `next/` in the correct build order.

## Purpose

Provide a staging area for ideation and refinement before formalizing into numbered plans.

## In‑scope

- Capturing problems, contexts, and acceptance criteria
- Pruning or merging items to reduce overhead

## Out‑of‑scope

- Numbered plans (PLAN-<id>_<slug>.md); those belong in `next/`
- Manual moves without review or rationale

## Workflows

- Review backlog:
  - Curate, merge duplicates, prune outdated items.
- Promote to numbered plan and move to next/:
  - Assign a plan_id and slug.
  - Move file to `docs/project/next/` following naming rules.

## Rules

- Backlog files should not use `PLAN-<id>_<slug>.md` names.
- Keep items concise and decision-ready (≤ 1 screen ideal).
- Prefer pruning; you can always resurrect from git history.

## Checks & Gates

- Policy/readmes sanity (informational here):
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- Plan lifecycle hygiene (Plan 50):
  ```
  nexoscli dev plans verify --output json
  nexoscli dev plans index build --output json
  nexoscli dev doctor --ci --checks plans --output json
  ```
  Note: backlog is advisory; the plans check reports warnings if numbered PLAN-* appears here.

## Interfaces

- Inputs: unnumbered backlog entries
- Outputs: numbered plans promoted to `next/`

## Backlog Index

<!-- BEGIN: AUTO (id=plans-index) -->
- [v0_replay_assesment.md](./v0_replay_assesment.md)
<!-- END: AUTO -->
