# FileOps Schema v0.1 (Reserved)

Canonical schema tag: "nexos/fileops@0.1"

Purpose
- Define a canonical line format for repository file operation events correlated to a run.
- Enable validation of `fileops.ndjson` artifacts in TracePack bags.
- Keep v0.1 reserved for Plan-35 (fileops normalizer); this doc anchors the JSON Schema and retrieval CLI now.

Line format (NDJSON)
Each line is one JSON object with required and optional fields.

Required fields
- op: string — one of `create | modify | delete | rename`
- path: string — repo-relative path

Recommended/optional fields
- schema: string — `"nexos/fileops@0.1"`
- ts: string — RFC3339 timestamp
- run_id: string — ID of the run/session
- bytes: int ≥ 0 — size after operation (if known)
- digest_after: string — `sha256:...` content digest after operation
- patch.sha256: string — `sha256:...` for the patch payload when present
- patch.lines: int ≥ 0 — lines in patch
- source: string — e.g., `"term.run"`
- context: object — optional tracking fields (task_id, session_id, stage, ...)

Notes
- Additional properties are allowed to avoid breaking older logs.
- Validation is tolerant to extra fields but requires the core `op` and `path`.
- The normalization/emission of fileops will land in Plan-35.

Example line
```json
{
  "schema": "nexos/fileops@0.1",
  "ts": "1970-01-01T00:00:00Z",
  "run_id": "RUN-ABC123",
  "op": "modify",
  "path": "src/main.rs",
  "bytes": 1024,
  "digest_after": "sha256:…",
  "patch": { "sha256": "sha256:…", "lines": 42 },
  "source": "term.run",
  "context": { "task_id": "TASK-1" }
}
```

Retrieve JSON Schema (CLI)
```bash
# List available schemas
nexoscli replay schema list --output json

# Export the fileops v0.1 JSON Schema to a file
nexoscli replay schema get --name fileops --version 0.1 --out ./fileops.schema.json
```

Validate fileops (when present)
```bash
# Validate a directory-based bag produced by `replay extract`
nexoscli replay validate --bag ./bag-dir --what fileops --output json
```
- If the bag does not contain fileops, validation will skip it (v0).
