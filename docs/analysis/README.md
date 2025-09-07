---
schema: readme.v0
title: "Analysis/"
summary: "Deterministic, single-file EntrySpecs and report templates for repeatable analyses."
kind: domain
owner: "@docs/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-07T00:00:00Z"
parents: ["../README.md","../../README.md"]
children: []
tags: ["analysis","templates","agents"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# Analysis — EntrySpec System

Use **ENTRY.md** to drive a deterministic analysis. Typical outputs:
- `00-methodology.md`
- `01-inventory.md`
- `02-systems-and-relations.md`
- `03-cli-capabilities.md`
- `04-findings-gaps-recos.md`

## How to start a new analysis
1. Copy `_templates` → `./<yyyy-mm>_<slug>/`  
2. Rename `ENTRY.template.md` → `ENTRY.md` and fill front‑matter  
3. Run the Cline workflow: see `docs/prompts/cline/execute_entry.workflow.md`
