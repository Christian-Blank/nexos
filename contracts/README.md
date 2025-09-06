# Contracts (Schemas) Provenance

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
