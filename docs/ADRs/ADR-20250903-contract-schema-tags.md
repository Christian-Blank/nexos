# ADR-20250903 — Schema tags & schema export

Status: Accepted

Context:
We need a single source of truth for public contracts (events, bag manifest, term records, file watch). Hand-maintained JSON Schemas tend to drift from reality. We also want every emitted JSON record to carry a stable schema tag for downstream validation and evolution.

Decision:
1) Add a "schema" string field to every JSON record the CLI emits (events, bag manifest, term record, file-watch).
2) Derive JSON Schemas from the actual Rust types via schemars and expose `nexoscli dev schemas export`.
3) Treat schema changes as contracts: additive changes are minor; breaking changes require an upconverter and a new schema version string.

Consequences:
- Protected files touched: minimal additive change to ensure "schema" is present in all event lines (legacy non-ledger write path).
- Replay plugin will add top-level "schema" to bag manifest.
- A contracts module will define the types used for schema export and re-exported via core.

Versioning policy:
- Events: "event.v0.3"
- Term record: "term_record.v0.1"
- File watch event: "file_watch_event.v0.1"
- Bag manifest: "bag_manifest.v0.2"

Migration and Compatibility:
- The new "schema" field is additive. Consumers ignoring unknown fields continue to work.
- Schemas are generated from Rust types, eliminating drift between docs and code.
- For any breaking change, bump the schema version string and add an upconverter in replay tooling.

Verification:
- A new E2E test (`dev_schemas_export.rs`) asserts the presence and JSON parseability of exported schema files.
- Manual checks: building a bag shows `schema: "bag_manifest.v0.2"` in manifest.json; events in `.nexoscli/log/events.jsonl` include `"schema": "event.v0.3"` (both with ledger on and off).
