---
schema: readme.v0
title: "docs/"
summary: "Core documentation and decision records for the NexOS CLI project."
kind: domain
owner: "@docs/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "../docs/ADRs/"
in_scope:
  - "Repository documentation, ADRs, playbooks, and workflows"
  - "Execution graph and plan lifecycle docs"
out_of_scope:
  - "Executable code (docs contains no code per policy)"
inputs:
  - "Plan files and ADRs"
outputs:
  - "Rendered docs and navigation structure"
tags: ["docs"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---

# NEXOS Documentation

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated children list from front‑matter or glob._
<!-- END: AUTO -->
Core documentation and decision records for the NEXOS CLI project.

## Navigation

**Foundation Documents:**
- [`00-project-brief.md`](00-project-brief.md) - Project goals and scope  
- [`01-north-star.md`](01-north-star.md) - Principles, budgets, and non-goals
- [`exec_graph.yaml`](exec_graph.yaml) - Authoritative execution DAG

**Process & Development:**
- [`PLAYBOOK.md`](PLAYBOOK.md) - Working agreements and development workflow

**Architecture Decisions:**
- [`ADRs/`](ADRs/) - Architecture Decision Records (contract changes)

**Contracts & Schemas:**
- [contracts/schemas/README.md](../contracts/schemas/README.md) - Generated JSON Schemas from real types (do not hand-edit)
- Regenerate: `nexoscli dev schemas export --out contracts/schemas`

**Planning & Plans:**
- [`project/README.md`](project/README.md) — Plan lifecycle directories and CLI usage
- Workflows:
  - Start: [`prompts/cline/start_new_task.workflow.md`](prompts/cline/start_new_task.workflow.md)
  - Finalize: [`prompts/cline/finalize_task.workflow.md`](prompts/cline/finalize_task.workflow.md)

Quick Plan Lifecycle (CLI-native):
```
nexoscli dev init --yes
nexoscli dev plans start --output json
# implement work…
nexoscli dev plans finalize --output json
```

## For AI Agents

Parse YAML front-matter in each document as the authoritative record. The markdown body provides human context. Build execution DAG from `exec_graph.yaml` and follow dependency chains.

**Entry sequence:** North Star → Project Brief → Execution Graph → relevant ADRs

## Context

This is part of the NEXOS CLI project. See [parent README](../README.md) for complete system overview.
