---
schema: readme.v0
title: "project/dumped"
summary: "Discarded plans at any lifecycle stage; kept for historical digging."
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
  - "Parking place for abandoned/ditched plans"
  - "Historical digging when signal indicates value"
out_of_scope:
  - "Active prioritization (use backlog/) or lifecycle (next/current/done)"
inputs:
  - "Plans moved here when no longer pursued"
outputs:
  - "Historical artifacts for reference; may be resurrected"
tags: ["plans","dumped"]
auto:
  children_glob: []
  dir_tree_depth: 0
---

# Dumped

Folder for any plans that were abandoned at any stage of their life‑cycle. Keep the backlog crisp and effective; when in doubt, dump rather than hoard. You can always re‑dig with git history if value emerges.

## Purpose

Reduce cognitive overhead by pruning non‑essential plans while retaining history for future analysis.

## In‑scope

- Moving any plan here when its value proposition no longer holds
- Brief header notes explaining rationale (optional)

## Out‑of‑scope

- Treating dumped items as active candidates (use backlog/ instead)
- CI gating for these documents

## Workflows

- Move a plan to dumped:
  - In the same change, add a short rationale at the top of the plan.
- Resurrect from dumped:
  - Move to `backlog/`, refine, and re‑evaluate; only number when ready for `next/`.

## Rules

- Prefer dumping to reduce noise; rely on git for recovery.
- Do not include dumped items in prioritization flows.

## Checks & Gates

- Repo policy/readmes (informational here):
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- Plan lifecycle hygiene (Plan 50, informational here):
  ```
  nexoscli dev plans verify --output json
  nexoscli dev plans index build --output json
  nexoscli dev doctor --ci --checks plans --output json
  ```
- Not part of lifecycle gating.

## Interfaces

- Inputs: de‑prioritized plan files
- Outputs: historical context for potential future reuse

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
_No plan files currently listed in this folder._
<!-- END: AUTO -->
