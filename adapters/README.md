---
schema: readme.v0
title: "adapters/"
summary: "External system adapters and worker bridges (e.g., Temporal worker)."
kind: domain
owner: "@adapters/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "../docs/ADRs/"
in_scope:
  - "Adapters and bridges integrating external systems"
  - "Operational guidance and ownership"
out_of_scope:
  - "Core contracts and CLI internals (see nexoscli-core)"
inputs:
  - "External system events/configuration"
outputs:
  - "Adapter-facing events and worker processes"
tags: ["adapters"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# adapters/

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of adapter READMEs (e.g., temporal-worker)._
<!-- END: AUTO -->

**Purpose:** What lives here and why.

**Allowed content:** Keep this folder focused. Add examples (bullets).

**Prohibited content:** List what must *not* live here (with rationale).

**Ownership / DRI:** Fill in team/owner.

**Related generators:** Document any generators that populate this folder.

> This README was scaffolded by `nexoscli dev repo readmes --apply`.
