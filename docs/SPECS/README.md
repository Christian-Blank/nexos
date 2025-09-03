# Contract Schemas (v0)

These files are generated from real Rust types using schemars. Do not hand-edit. Regenerate via the CLI and commit any drift.

How to regenerate:
- cargo run -p nexoscli -- dev schemas export --out docs/SPECS

Generated files:
- event_v0.3.schema.json — event lines in .nexoscli/log/events.jsonl (hash-chained; schema const: "event.v0.3")
- term_record_v0.1.schema.json — term command record (if/when emitted)
- file_watch_event_v0.1.schema.json — file watch event (if/when emitted)
- bag_manifest_v0.2.schema.json — NEXBag/TracePack manifest (manifest.json)

Notes:
- Schemas are derived from nexoscli-core/src/contracts.rs (and related types). This ensures a single source of truth and prevents drift.
- All emitted JSON records include a "schema" field as a stable tag per ADR-20250903.
