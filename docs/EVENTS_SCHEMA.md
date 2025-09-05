# Events Schema v0.3

Canonical schema tag: "nexos/events@0.3"

Purpose
- Establish a stable, analysis-ready event record independent of protected core files.
- Enable normalization from legacy logs/bags to a canonical shape while preserving unknown fields.
- Provide deterministic hash semantics for integrity and chain verification.

Line format (NDJSON)
Each line is one JSON object with required and optional fields.

Required fields
- schema: string — fixed "nexos/events@0.3"
- ts: string — RFC3339 timestamp
- plugin: string — emitting plugin name (e.g., "term", "issue")
- command: string — command within the plugin (e.g., "run", "new")
- exit_code: int — process exit code (0 when N/A)
- run_id: string — ID of the run/session this event belongs to
- seq: uint64 — sequence index within the normalized stream (1-based)
- content_hash: string — sha256 over canonical subset (see below)
- chain_hash: string — sha256(prev + content_hash) where prev is previous chain head or empty for first

Optional fields
- dur_ms: uint64 — duration in milliseconds
- args_hash: string — sha256 over arguments (when applicable)
- prev: string — previous chain head; omitted/null for the first event
- data: object — event-specific data payload
- meta: object — auxiliary metadata
- attrs: object — container for unknown/non-standard fields from source lines; may include convenience tags like "source": "plugin.command"

Canonical hashing
- content_hash = sha256( JSON.stringify({ plugin, command, exit_code, dur_ms, data }) )
  - Canonical object keys as listed
  - JSON serialization is stable via serde/serde_json
- chain_hash = sha256(prev + content_hash)
  - prev is the previous event’s chain_hash (empty string for the first event)
  - Ensures tamper-evident chaining

Sequence semantics
- seq starts at 1 and increments by one per event in output order for the normalized stream.
- When normalizing shared logs with --run-id filter, numbering applies to the filtered sequence only.

Normalization rules
When transforming legacy or existing lines to v0.3:
- Required fields must be present: ts, plugin, command, exit_code (default 0 if missing), run_id.
- seq is assigned by the normalizer if absent.
- dur_ms, args_hash, data, meta are copied if present.
- Unknown top-level fields are moved under attrs to avoid loss of information.
- attrs.source = "plugin.command" is set as a convenience for analysts.
- content_hash and chain_hash are recomputed using the canonical rules.
- prev is set to the previous chain head; omitted/null for the first line.

Example line
```json
{
  "schema": "nexos/events@0.3",
  "ts": "1970-01-01T00:00:00Z",
  "plugin": "term",
  "command": "run",
  "exit_code": 0,
  "dur_ms": 1250,
  "run_id": "RUN-ABC123",
  "seq": 1,
  "args_hash": null,
  "prev": null,
  "content_hash": "sha256:…",
  "chain_hash": "sha256:…",
  "data": {
    "cmd": "make test",
    "cwd": "/repo"
  },
  "meta": null,
  "attrs": {
    "source": "term.run",
    "legacy_field": "retained"
  }
}
```

CLI surfaces
- Normalize to v0.3:
  - nexoscli replay normalize --to 0.3 [--bag PATH | --events PATH] [--run-id RUN-…] [--out PATH] [--update-manifest]
  - Output: events.v0.3.ndjson (default) or path from --out
  - --run-id filters when normalizing a shared events log
  - --update-manifest adds "events_v03" to bag manifest (dir only)

- Print v0.3 schema:
  - nexoscli replay schema events --version 0.3 --output json
  - Returns schemars-generated JSON Schema and an inlined example

Manifest linkage (optional)
When --update-manifest is used with --bag (directory), the bag’s manifest.json gains:
```json
"events_v03": {
  "schema": "nexos/events@0.3",
  "dst": "events.v0.3.ndjson",
  "lines": 123,
  "bytes": 4567,
  "sha256": "sha256:…"
}
```

Notes
- Unknown/legacy fields are never dropped; they are preserved under attrs.
- Missing optional fields are omitted.
- Hashing is deterministic and read-only; normalization does not modify inputs.

## Retrieve JSON Schema (CLI)
You can list and export the canonical JSON Schema embedded in the CLI:
```bash
# List available schemas
nexoscli replay schema list --output json

# Export the events v0.3 JSON Schema to a file
nexoscli replay schema get --name events --version 0.3 --out ./events.schema.json
```

## Validate bags against schemas (v0)
Validate a directory-based bag produced by `replay extract`:
```bash
nexoscli replay validate --bag ./bag-dir --output json
```
- Reports per-artifact results with counts and up to 10 concise errors.
- Directory bags only in v0 (untar archives first if needed).
