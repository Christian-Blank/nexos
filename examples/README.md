---
schema: readme.v0
title: "examples/"
summary: "Executable examples and workflow demos for NexOS."
kind: domain
owner: "@examples/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./"
in_scope:
  - "Runnable examples and demonstration workflows"
  - "Reference configurations for users and DRIs"
out_of_scope:
  - "Core contracts and CLI internals (see nexoscli-core)"
inputs:
  - "CLI commands and workflow specs"
outputs:
  - "Example READMEs and runnable demos"
tags: ["examples"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# examples/

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of example READMEs and workflow demos._
<!-- END: AUTO -->

**Purpose:** What lives here and why.

**Allowed content:** Keep this folder focused. Add examples (bullets).

**Prohibited content:** List what must *not* live here (with rationale).

**Ownership / DRI:** Fill in team/owner.

**Related generators:** Document any generators that populate this folder.

> This README was scaffolded by `nexoscli dev repo readmes --apply`.
