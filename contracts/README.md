---
schema: readme.v0
title: "contracts/"
summary: "Frozen JSON Schemas for NexOS public contracts; generated deterministically from Rust types."
kind: domain
owner: "@contracts/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: ["./schemas/README.md"]
source_of_truth:
  - "./schemas/"
in_scope:
  - "Contract schema snapshots and provenance"
  - "Regeneration and drift verification guidance"
out_of_scope:
  - "Hand-editing schemas (use code + export tooling)"
inputs:
  - "Rust contract types, CLI export command"
outputs:
  - "Versioned JSON Schemas under contracts/schemas/"
tags: ["contracts","schemas"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# Contracts (Schemas) Provenance

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of contract subfolders (e.g., schemas/)._
<!-- END: AUTO -->

This directory contains frozen JSON Schemas for Nexos public contracts. These files are generated from real Rust types (via schemars) and exported by the CLI. Do not hand-edit any of these files.

- Canonical location: `contracts/schemas/`
- Source of truth: Rust contract types in the codebase
- Generation tool: `nexoscli dev schemas export`
- Policy: Drift-free snapshots; changes must come from code and be exported deterministically

## Regenerate Schemas

Use the Justfile recipes (preferred) or call the CLI directly.

With just:
- `just regen-schemas`

Direct CLI:
- `cargo run -q -p nexoscli -- dev schemas export --out contracts/schemas`

## Verify for Drift

With just:
- `just verify-schemas`

Under the hood this exports to a temp directory and recursively diffs against `contracts/schemas`. Any difference means the contract types changed and the snapshots must be updated in a dedicated commit with the appropriate task/ADR reference.

Equivalent manual check:
- `tmp="$(mktemp -d)"; cargo run -q -p nexoscli -- dev schemas export --out "$tmp"; diff -ru "$tmp" contracts/schemas`

## CI Guidance

- Never commit manual edits to `contracts/schemas/**`
- CI may run:
  - `cargo run -q -p nexoscli -- dev schemas export --out contracts/schemas`
  - `git diff --exit-code -- contracts/schemas`
- Breaking or tightening changes should be accompanied by an ADR and conventional commit message referencing the Task/ADR IDs.

## Notes

- Events schema: `event_v0.3.schema.json`
- File watch schema: `file_watch_event_v0.1.schema.json`
- Term record schema (reserved): `term_record_v0.1.schema.json`
- Bag manifest: `bag_manifest_v0.2.schema.json`

These are consumed by tools and docs; ensure all paths reference `contracts/schemas` (legacy schema locations under docs were removed).
