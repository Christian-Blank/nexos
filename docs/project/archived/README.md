---
schema: readme.v0
title: "project/archived"
summary: "Retired or abandoned plans (optional) retained for historical context."
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
  - "Archival of plans no longer pursued"
  - "Historical references not part of active lifecycle"
out_of_scope:
  - "Active plan lifecycle and CI gating"
inputs:
  - "Plans moved here via review/CP (not finalize)"
outputs:
  - "Historical record of retired approaches"
tags: ["plans","archived"]
auto:
  children_glob: []
  dir_tree_depth: 0
---

# Archived

Retired or abandoned plans kept for context. These are not part of the active lifecycle and are not considered in gating.

## Purpose

Preserve prior approaches and decisions that were intentionally not pursued or superseded.

## In‑scope

- Storing abandoned/retired plan documents
- Linking to superseding plans or ADRs

## Out‑of‑scope

- Active plan management
- CI gating for current work

## Workflows

- Move a plan to archived (governed by CP/ADR and review):
  - Open a Change Proposal or ADR referencing the plan being archived.
  - Move the file to `docs/project/archived/` in the same change.
  - Add links/rationale in the plan header if needed.

## Rules

- Archived plans should not be treated as active work items.
- Do not mutate archived plan content except to add a short header note with rationale and links to superseding work.

## Checks & Gates

- Repo policy and readmes (informational for this folder):
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- Plan lifecycle hygiene (Plan 50, informational here):
  ```
  nexoscli dev plans verify --output json
  nexoscli dev plans index build --output json
  nexoscli dev doctor --ci --checks plans --output json
  ```
- Not part of lifecycle CI gates.

## Interfaces

- Inputs: retired plan files
- Outputs: historical references and links

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
_No plan files currently listed in this folder._
<!-- END: AUTO -->
