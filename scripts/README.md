---
schema: readme.v0
title: "scripts/"
summary: "Utility scripts and CI helpers used across the repository."
kind: domain
owner: "@scripts/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./"
in_scope:
  - "Developer and CI utility scripts"
  - "Automation helpers referenced by docs and CI"
out_of_scope:
  - "Executable code living under docs/ (policy forbids code in docs)"
inputs:
  - "Workflow and CI requirements"
outputs:
  - "Reusable scripts and documentation"
tags: ["scripts","ci"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# scripts/

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of script subfolders if present._
<!-- END: AUTO -->

**Purpose:** What lives here and why.

**Allowed content:** Keep this folder focused. Add examples (bullets).

**Prohibited content:** List what must *not* live here (with rationale).

**Ownership / DRI:** Fill in team/owner.

**Related generators:** Document any generators that populate this folder.

> This README was scaffolded by `nexoscli dev repo readmes --apply`.
