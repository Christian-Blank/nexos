---
schema: readme.v0
title: "rules/"
summary: "Repository rules, proposals, and templates governing contributions and checks."
kind: domain
owner: "@rules/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./"
in_scope:
  - "Rules, proposals, and templates for repository governance"
  - "Inputs to repo checks and guardrails"
out_of_scope:
  - "Executable code (see policy: no code under docs/)"
inputs:
  - "Policy docs and ADRs"
outputs:
  - "Rules definitions and templates"
tags: ["rules"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# rules/

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of rule-related subfolders (e.g., pack/, proposals/, templates/)._
<!-- END: AUTO -->

**Purpose:** What lives here and why.

**Allowed content:** Keep this folder focused. Add examples (bullets).

**Prohibited content:** List what must *not* live here (with rationale).

**Ownership / DRI:** Fill in team/owner.

**Related generators:** Document any generators that populate this folder.

> This README was scaffolded by `nexoscli dev repo readmes --apply`.
