---
schema: readme.v0
title: "testdata/"
summary: "Static test fixtures and datasets used for local and CI validation."
kind: domain
owner: "@tests/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./"
in_scope:
  - "Deterministic test fixtures and datasets"
  - "Golden files for contract and replay validation"
out_of_scope:
  - "Executable scripts (see ../scripts)"
inputs:
  - "Schemas, events, CLI outputs"
outputs:
  - "Reusable fixtures consumed by tests and tools"
tags: ["tests","fixtures"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# testdata/

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of testdata subfolders if present._
<!-- END: AUTO -->

Provide stable, deterministic fixtures for local and CI usage. Keep files small and well-labeled. Large data sets should be generated or referenced, not committed.
