# TracePack — Portable Run Bundle (v0.2)

A TracePack is a **self-contained** snapshot of a single `nexoscli` run:
- canonical **events** (NDJSON, schema v0.2),
- any **patches** captured by `term run`,
- a **manifest** with hashes and counts,
- optional status snapshots.

## What’s new in v0.2

- Ledger anchors: TracePacks now include a per‑run ledger manifest (run_manifest.json) derived from the tamper‑evident event chain (event.v0.3).
- Integrity flag: The manifest includes chain_ok (boolean) indicating whether the hash chain verified at extraction time.
- Backward compatibility: Layout remains the same; existing consumers can ignore run_manifest.json until they adopt verification.

Note: The replay extractor will populate run_manifest.json and set chain_ok via the verifier (planned in Replay phase).

## Produce a TracePack

```bash
# directory
nexoscli replay extract --run-id RUN-... --out .nexoscli/tracepacks/pack-1

# tarball
nexoscli replay extract --run-id RUN-... --out tracepack.tar.gz

# events only
nexoscli replay extract --run-id RUN-... --out events-pack --include events

# patches and status only
nexoscli replay extract --run-id RUN-... --out patches-pack --include patches,status
```

## Layout

```
manifest.json          # metadata, counts, hashes
events.ndjson          # newline-delimited events for the run
patches/               # git patches from term run
  seq-0001-<name>.patch
  seq-0002-<name>.patch
status/                # git status snapshots
  seq-0001-<name>.status
  seq-0002-<name>.status  
fileops/fileops.ndjson          # derived file operations (Plan-35)
README.md              # this documentation
```

## Analyze

### Events Analysis
```bash
# View all events in the pack
jq -r '.' tracepack/events.ndjson

# Count events by plugin
jq -r '.plugin' tracepack/events.ndjson | sort | uniq -c

# Find specific command executions
jq -r 'select(.plugin == "term" and .command == "run")' tracepack/events.ndjson

# Timeline analysis
jq -r '.ts + " " + .plugin + "." + .command' tracepack/events.ndjson | sort
```

### Manifest Analysis
```bash
# Summary statistics
jq '.counts' tracepack/manifest.json

# Plugin usage breakdown
jq '.plugins' tracepack/manifest.json

# Command frequency
jq '.commands' tracepack/manifest.json

# Time range
jq '.time_range' tracepack/manifest.json

# Git range
jq '.git' tracepack/manifest.json
```

### Patches Analysis
```bash
# Check patch applicability (dry-run)
cd /path/to/repo
git apply --check tracepack/patches/seq-0001-*.patch

# View patch contents
cat tracepack/patches/seq-0001-*.patch

# Apply patches on throwaway branch
git checkout -b test-replay
for patch in tracepack/patches/seq-*.patch; do
  git apply "$patch"
done
```

### File Operations (Plan-35)

When file-change topic (fs-topic) is not present, file operations are derived from patches:

- Derivation: replay extract scans patches/seq-*.patch unified diffs and classifies ops by headers:
  - "new file mode" → create
  - "deleted file mode" → delete
  - "rename from"/"rename to" → rename
  - otherwise → modify
- Output: fileops/fileops.ndjson is written and linked in manifest.json as fileops with counts.fileops_lines.
- Validation: replay validate resolves fileops/fileops.ndjson or fileops.ndjson.

Standalone derivation and validation:
```bash
# Derive fileops.ndjson from a bag directory (patches present)
nexoscli replay fileops derive --bag ./tracepack --out ./fileops.ndjson --output json

# Validate only fileops in a bag directory
nexoscli replay validate --bag ./tracepack --what fileops --output json
```

### CSV Export (analyst-friendly)

Export CSVs for quick spreadsheet analysis directly from a TracePack/NEXBag directory.

Usage:
```bash
# Export both events and file operations to CSV (defaults)
nexoscli replay export --bag ./tracepack --what all --as csv

# Export only events
nexoscli replay export --bag ./tracepack --what events --as csv

# Export only fileops
nexoscli replay export --bag ./tracepack --what fileops --as csv

# Choose a custom output directory
nexoscli replay export --bag ./tracepack --what all --as csv --out ./exports/csv
```

Defaults:
- what: all
- as: csv
- out: ./exports/csv under the bag (e.g., tracepack/exports/csv)

Outputs:
- tracepack/exports/csv/events.csv
- tracepack/exports/csv/fileops.csv

Path resolution (tolerant of legacy layouts):
- Events source: prefer tracepack/events.ndjson; fallback tracepack/topics/events.ndjson
- FileOps source: prefer tracepack/fileops/fileops.ndjson; fallback tracepack/fileops.ndjson

Notes:
- CSV headers are always written, even if there are zero rows.
- Empty/non-JSON lines in NDJSON are skipped; missing fields render as empty CSV cells.
- v0 supports directory bags; extract archives (.tgz) to a directory before exporting.

Columns:

events.csv
- ts, run_id, plugin, command, exit_code, dur_ms, error_code, args_hash, content_hash, task_id, stage, session_id, attrs_cmd, attrs_cwd, attrs_json

fileops.csv
- ts, run_id, op, path, from, to, bytes, digest_after, patch_sha256, patch_lines, source

## Integration with CLI Workflows

### 1. Development Debugging
```bash
# Generate activity
nexoscli term run -- make build
nexoscli term run -- make test

# Find the failing run
nexoscli dev telemetry report --since 1h --output json | \
  jq '.data.runs[] | select(.exit_code != 0)'

# Extract for analysis
nexoscli replay extract --run-id RUN-FAILED-123 --out debug-pack.tgz

# Share with team
scp debug-pack.tgz teammate:~/debug/
```

### 2. CI/CD Analysis
```bash
# Extract CI runs for performance analysis
nexoscli replay extract --run-id $CI_RUN_ID --out ci-trace.tgz

# Automated analysis pipeline
tar -xzf ci-trace.tgz
jq '.counts.events_lines' manifest.json  # activity volume
jq '.plugins' manifest.json | jq 'keys[]' # tools used
```

### 3. Audit and Compliance
```bash
# Extract security-relevant runs
nexoscli replay extract --run-id SECURITY-SCAN-456 --out audit/scan-456.tgz

# Verify integrity
jq -r '.patches[] | .sha256' manifest.json | while read hash; do
  echo "Verifying $hash..."
done
```

## TracePack Manifest Schema

```json
{
  "version": "0.1",
  "kind": "tracepack", 
  "run_id": "RUN-UUID-HERE",
  "created_at": "2025-09-02T07:30:00Z",
  "time_range": {
    "from": "2025-09-02T07:29:15Z",
    "to": "2025-09-02T07:29:45Z"
  },
  "counts": {
    "events_lines": 42,
    "steps": 3,
    "patches": 2,
    "status": 2
  },
  "git": {
    "from": "HEAD~1",
    "to": "HEAD"
  },
  "plugins": {
    "term": 2,
    "issue": 1
  },
  "commands": {
    "term.run": 2,
    "issue.new": 1
  },
  "patches": [
    {
      "seq": 1,
      "src": ".nexoscli/patches/20250902T073000Z-RUN-UUID-HERE.patch",
      "dst": "patches/seq-0001-20250902T073000Z-RUN-UUID-HERE.patch", 
      "sha256": "sha256:abcd1234...",
      "bytes": 1024
    }
  ],
  "status_files": [
    {
      "seq": 1,
      "src": ".nexoscli/status/20250902T073000Z-RUN-UUID-HERE.status",
      "dst": "status/seq-0001-20250902T073000Z-RUN-UUID-HERE.status",
      "sha256": "sha256:efgh5678...", 
      "bytes": 256
    }
  ]
}
```

## Event Schema (v0.2)

Each line in `events.ndjson` follows this schema:

```json
{
  "ts": "2025-09-02T07:29:30Z",
  "run_id": "RUN-UUID-HERE", 
  "plugin": "term",
  "command": "run",
  "args_hash": "sha256:...",
  "content_hash": "sha256:...",
  "exit_code": 0,
  "dur_ms": 1250,
  "attrs": {
    "cmd": "make test",
    "cwd": "/path/to/project"  
  }
}
```

## Advanced Analysis Examples

### Performance Profiling
```bash
# Extract slow commands across multiple runs
for run_id in $(nexoscli dev telemetry report --since 1d --output json | \
  jq -r '.data.runs[].run_id'); do
  nexoscli replay extract --run-id $run_id --out tmp-$run_id --include events
  jq -r 'select(.dur_ms > 5000) | [.ts, .plugin, .command, .dur_ms] | @tsv' \
    tmp-$run_id/events.ndjson
done
```

### Error Correlation
```bash
# Find patterns in failed runs
nexoscli replay extract --run-id FAILED-RUN --out error-analysis
jq -r 'select(.exit_code != 0) | [.ts, .plugin, .command, .exit_code] | @csv' \
  error-analysis/events.ndjson > errors.csv
```

### Dependency Analysis
```bash
# Track which tools are used together
jq -r '.plugins | keys[]' */manifest.json | sort | uniq -c | sort -nr
```

## Storage and Organization

### Recommended Directory Structure
```
.nexoscli/
└── tracepacks/
    ├── daily/
    │   ├── 2025-09-01/
    │   │   ├── RUN-MORNING-123.tgz
    │   │   └── RUN-AFTERNOON-456.tgz
    │   └── 2025-09-02/
    └── projects/
        ├── nexos-cli/
        │   ├── build-traces/
        │   └── test-traces/ 
        └── other-project/
```

### Automation Scripts
```bash
#!/bin/bash
# auto-archive-traces.sh
# Archive TracePacks older than 30 days

find .nexoscli/tracepacks -name "*.tgz" -mtime +30 | \
  xargs -I {} mv {} .nexoscli/tracepacks/archive/
```

## Integration with Replay Commands

### Planning and Dry-Run
```bash
# Extract a TracePack
nexoscli replay extract --run-id RUN-123 --out analysis-pack

# Plan replay steps 
nexoscli replay plan --run-id RUN-123

# Dry-run patches
nexoscli replay run --run-id RUN-123 --dry-run --branch test-branch
```

### Safe Patch Application
```bash
# Create throwaway branch
git checkout -b replay-test

# Apply patches from TracePack
nexoscli replay run --run-id RUN-123 --branch replay-test

# Review results
git log --oneline replay-test ^main
git diff main..replay-test

# Clean up if needed
git checkout main && git branch -D replay-test
```

## Best Practices

1. **Regular Extraction**: Extract TracePacks for important runs immediately
2. **Naming Convention**: Use descriptive names: `feature-x-test-run.tgz`
3. **Security**: TracePacks may contain sensitive data; store securely
4. **Retention**: Establish policies for TracePack retention and archival
5. **Sharing**: Use compressed format (`.tgz`) for sharing and storage efficiency
6. **Verification**: Always verify patch applicability before applying

## Troubleshooting

### Empty TracePacks
If a TracePack has zero events:
- Verify the run_id exists: `nexoscli dev telemetry report --since 7d`
- Check event log location: `.nexoscli/log/events.jsonl`
- Ensure the run completed successfully

### Patch Application Failures
If patches won't apply:
- Check git status: patches assume clean working directory
- Verify base commit: use git range from manifest
- Try on correct branch: patches may depend on specific branch state

### Large TracePacks
For very large TracePacks:
- Use `--include events` to extract only events
- Consider time-based filtering (future feature)
- Compress with higher compression ratios

## Cline Tasks (optional)

If you pass `--include cline` (or `--include all`), the extractor selects normalized Cline tasks
from `.nexoscli/cline/tasks/` whose modified time falls within the run's time window (±5 minutes),
and copies them into:

```
cline/tasks/<createdAt>-<id>.json
```

The `manifest.json` includes an array `cline_tasks` with:
- `id`, `dst`, `sha256`, `bytes`, `created_at`, `messages`
and a count at `counts.cline`.

### Usage Examples

```bash
# Extract with Cline tasks included
nexoscli replay extract --run-id RUN-... --include cline --out tracepack.tgz

# Extract everything including Cline tasks
nexoscli replay extract --run-id RUN-... --include all --out complete-pack

# Extract only Cline tasks (plus events for time window calculation)
nexoscli replay extract --run-id RUN-... --include events,cline --out cline-pack
```

### Extended Layout

```
manifest.json          # metadata, counts, hashes
events.ndjson          # newline-delimited events for the run
patches/               # git patches from term run
  seq-0001-<name>.patch
  seq-0002-<name>.patch
status/                # git status snapshots
  seq-0001-<name>.status
  seq-0002-<name>.status
cline/tasks/           # Cline tasks from time window
  2025-09-01T12-00-00Z-task_123.json
  2025-09-01T12-15-30Z-task_456.json
README.md              # this documentation
```

### Extended Manifest Schema

```json
{
  "version": "0.1",
  "kind": "tracepack", 
  "run_id": "RUN-UUID-HERE",
  "created_at": "2025-09-02T07:30:00Z",
  "time_range": {
    "from": "2025-09-02T07:29:15Z",
    "to": "2025-09-02T07:29:45Z"
  },
  "counts": {
    "events_lines": 42,
    "steps": 3,
    "patches": 2,
    "status": 2,
    "cline": 2
  },
  "git": {
    "from": "HEAD~1",
    "to": "HEAD"
  },
  "plugins": {
    "term": 2,
    "issue": 1
  },
  "commands": {
    "term.run": 2,
    "issue.new": 1
  },
  "patches": [...],
  "status_files": [...],
  "cline_tasks": [
    {
      "id": "task-123",
      "dst": "cline/tasks/2025-09-01T12-00-00Z-task_123.json",
      "sha256": "sha256:cdef9012...",
      "bytes": 2048,
      "created_at": "2025-09-01T12:00:00Z",
      "messages": 4
    },
    {
      "id": "task-456", 
      "dst": "cline/tasks/2025-09-01T12-15-30Z-task_456.json",
      "sha256": "sha256:3456abcd...",
      "bytes": 1536,
      "created_at": "2025-09-01T12:15:30Z", 
      "messages": 6
    }
  ]
}
```

### Cline Task Analysis

```bash
# View Cline task summaries
jq -r '.cline_tasks[] | [.id, .created_at, .messages] | @tsv' tracepack/manifest.json

# Extract task content for analysis
jq '.' tracepack/cline/tasks/*.json

# Count messages per task
jq -r '.cline_tasks[] | "\(.id): \(.messages) messages"' tracepack/manifest.json

# Find tasks with high message counts
jq -r '.cline_tasks[] | select(.messages > 10) | .id' tracepack/manifest.json

# Timeline correlation with CLI events
jq -r '.ts + " CLI: " + .plugin + "." + .command' tracepack/events.ndjson > timeline.txt
jq -r '.cline_tasks[] | .created_at + " CLINE: " + .id' tracepack/manifest.json >> timeline.txt
sort timeline.txt
```

### Time Window Behavior

The Cline task inclusion uses a **±5 minute safety pad** around the run's time window:

1. **Run time window**: Determined from first and last event timestamps
2. **Padded window**: Extended by 300 seconds (5 minutes) in both directions  
3. **File filtering**: Tasks are included if their `mtime` falls within the padded window
4. **Fallback strategy**: If JSON parsing fails or timestamps are missing, `mtime` is used

This ensures tasks adjacent to CLI runs are captured even with minor clock skew or write delays.

**Tip**: Run `nexoscli cline tail --exit-after 10` while working to keep normalized tasks fresh.

---
 
TracePacks enable **portable observability** - capture, share, and analyze any CLI run as a self-contained bundle.

## File-change Topic (opt-in)

When enabled, `term run` can record on-disk changes during a run and link them deterministically to the run via a small event.

Enable:
```bash
nexoscli term run --fs-topic [--fs-root PATH...] [--fs-exclude GLOB...] [--fs-hash] -- \
  sh -c 'echo hi > a.txt && mv a.txt b.txt && rm b.txt'
```

Watcher output (`.nexoscli/fileops/fileops-<ts>-<pid>.ndjson`):
```
{"type":"meta","phase":"start","ts":"...","roots":["."],"exclude":[".git/**",".nexoscli/**","target/**","node_modules/**"]}
{"schema":"file_watch_event.v0.1","op":"create","path":"src/lib.rs","size":1234,"sha256":"sha256:...","ts":"..."}
{"schema":"file_watch_event.v0.1","op":"modify","path":"README.md","size":902,"ts":"..."}
{"schema":"file_watch_event.v0.1","op":"rename","from":"a.txt","to":"b.txt","ts":"..."}
{"schema":"file_watch_event.v0.1","op":"delete","path":"b.txt","ts":"..."}
{"type":"meta","phase":"end","ts":"...","total":4}
```

Linking event (written automatically on completion):
```
plugin: "term", command: "fs.topic"
data: { "file": ".nexoscli/fileops/fileops-…ndjson", "lines": N, "bytes": B, "sha256": "sha256:…", "from_ts":"…", "to_ts":"…" }
```

TracePack inclusion (`replay extract`):
- The topic file is copied to `fileops/fileops.ndjson`.
- The manifest adds `counts.fileops_lines` and a `fileops` object:
  ```
  {
    "dst": "fileops/fileops.ndjson",
    "sha256": "sha256:…",
    "bytes": B,
    "lines": N,
    "from": "…",
    "to": "…"
  }
  ```

Defaults and notes:
- Off by default; only active when `--fs-topic` is passed.
- Default excludes: `.git/**`, `.nexoscli/**`, `target/**`, `node_modules/**` (override with `--fs-exclude`).
- `--fs-hash` computes sha256 for `create`/`modify`; otherwise `sha256` is omitted (null).
- Line schema uses lower-case ops and conforms to `docs/SPECS/file_watch_event_v0.1.schema.json`.

## NEXBags (TracePack v0.2)

NEXBags are portable, analyzable bundles focused on topicized data with a bag-level manifest and integrity verification of the event ledger (event.v0.3). They are analogous to ROS bag files. The bag manifest includes a stable schema tag: "schema": "bag_manifest.v0.2".

Commands:
```bash
# Build a bag (directory or .tgz)
nexoscli replay bag build --run-id RUN-... --out ./tracebag                # dir
nexoscli replay bag build --run-id RUN-... --out ./tracebag.tgz            # tgz

# Verify bag integrity (events stats + hash-chain)
nexoscli replay bag verify --bag ./tracebag
nexoscli replay bag verify --bag ./tracebag.tgz

# Strict mode: exact time_range equality; require digests when present
nexoscli replay bag verify --bag ./tracebag --strict

# Cross-check against rotation index (Plan-29 segment-chain)
nexoscli replay bag verify --bag ./tracebag --index .nexoscli/log/events.index.json

# Show bag summary (counts, topics, versions, time_range)
nexoscli replay bag info --bag ./tracebag

# Cat a topic (bounded by --lines if provided)
nexoscli replay bag cat --bag ./tracebag --topic events
nexoscli replay bag cat --bag ./tracebag --topic events --lines 10
```

Bag verification checks
- manifest.json schema/kind sanity
- topics/events.ndjson: lines, bytes, sha256 over newline-joined lines, first/last timestamps
- optional artifacts: patches/status_files/cline_tasks presence and sha256 checks (when arrays present)
- optional continuity: recompute segment-chain head against .nexoscli/log/events.index.json

Machine output
The verify command emits a JSON envelope with:
- data.ok boolean
- data.events { lines, bytes, sha256_file, from_ts, to_ts }
- optional data.index_check { head_ok, issues[] } when --index is provided or auto-detected
- issues[] with kind = missing|mismatch|parse|warning

Exit codes
- 0: verified OK
- 2: verification failed (structural/digest mismatch)
- >2: operational errors (I/O, parse)

Layout (v0.2):
```
tracebag/
  manifest.json         # bag_manifest_v0.2
  run_manifest.json     # per-run manifest v0.2 snapshot
  topics/
    events.ndjson       # event_v0.3 lines
    term/commands.ndjson   # (reserved for Phase B)
    files/watch.ndjson     # (optional; reserved)
  cline/tasks/          # normalized Cline tasks within time window (±5m)
  README.md
```

Schemas:
- docs/SPECS/bag_manifest_v0.2.schema.json
- docs/SPECS/term_record_v0.1.schema.json (reserved for terminal capture in Phase B)
- docs/SPECS/file_watch_event_v0.1.schema.json (optional topic)

Notes:
- chain_ok is emitted by build and can be re-checked by verify.
- Event chain verification uses content_hash over the canonical subset and chain_hash(prev+content_hash), per event.v0.3.
- Cline tasks are included based on file mtime within a padded window around the run time range (±5 minutes).

## Verify & Explain

Use the top-level verifier to validate a TracePack (directory or .tgz) and get actionable explanations. Output is always wrapped in a stable JSON envelope suitable for CI.

Examples:
```bash
# Directory or .tgz
nexoscli replay verify --path tracepack/
nexoscli replay verify --path tracepack.tgz

# Also dry‑run check patches via git (best effort)
nexoscli replay verify --path tracepack/ --git-check

# Skip specific checks (comma-separated)
nexoscli replay verify --path tracepack/ --skip patch-apply,cline
```

What it checks:
- manifest.json presence and minimal schema sanity
- events.ndjson: strict JSON-per-line, single run_id, line count vs manifest
- fileops/fileops.ndjson (if advertised in manifest): sha/bytes/lines
- patches/*: presence and sha/bytes; optional git apply --check with --git-check
- status/*: presence and basic sanity
- cline/tasks/*: presence if advertised
- time_range consistency between manifest and first/last event timestamps (warn on slight inconsistencies)

Exit codes:
- 0: OK
- 2: Failure (structural or digest mismatch)

Tip: Pipe to jq for readability:
```bash
nexoscli replay verify --path tracepack/ --output json | jq
```

## Normalize events to v0.3

Normalize existing logs or TracePacks to the canonical events schema for downstream analysis.

Why
- Lock down a stable, analysis-ready shape: schema "nexos/events@0.3".
- Preserve unknown/legacy fields under attrs; omit missing optionals.
- Deterministic content/chain hashing, matching verification logic.

Usage
```bash
# Normalize a TracePack directory and update manifest with events_v03
nexoscli replay normalize --to 0.3 --bag ./tracepack --update-manifest

# Normalize a TracePack archive (.tgz) to an external file (read-only archive)
nexoscli replay normalize --to 0.3 --bag ./tracepack.tgz --out ./events.v0.3.ndjson

# Normalize a shared events log and filter by run_id
nexoscli replay normalize --to 0.3 --events .nexoscli/log/events.jsonl --run-id RUN-ABC123 --out ./events.v0.3.ndjson
```

Manifest linkage (when --update-manifest with a directory bag)
```json
"events_v03": {
  "schema": "nexos/events@0.3",
  "dst": "events.v0.3.ndjson",
  "lines": 123,
  "bytes": 4567,
  "sha256": "sha256:…"
}
```

Schema
- Print the JSON Schema and an inlined example:
```bash
nexoscli replay schema events --version 0.3 --output json
```

Notes
- Exactly one of --bag or --events is required.
- For .tgz inputs, --out is required; archives are not modified.
- The normalizer is read-only except when --update-manifest is specified for a directory bag.

### Annotations (v0)

Attach labels/notes to a bag, an event row, or a file operation row, and export/join them for analysis.

Bag files:
- annotations.ndjson — append-only NDJSON, one JSON object per line
- events.ndjson, fileops/fileops.ndjson — existing sources
- exports/csv/ — generated CSVs

Schema (v0, stable):
{
  "ts": "RFC3339 (creation time)",
  "source": "human|agent|rule|workflow",
  "author": "optional string",
  "labels": ["string", "..."],
  "note": "optional string",
  "confidence": 0.0,
  "target": {
    "kind": "event|fileop|bag",
    "seq": 12,                 // 0-based row index in events/fileops (required for event|fileop)
    "ts": "RFC3339",           // copied from target row if available
    "run_id": "RUN-...",
    "plugin": "term",          // events only (best-effort)
    "command": "run",          // events only (best-effort)
    "op": "write|move|...",    // fileops only (best-effort)
    "path": "a.txt"            // fileops only (best-effort)
  },
  "attrs": { "free": "shape" } // optional extras
}

Design: target.seq is the v0 join key. It is stable because events.csv/fileops.csv preserve NDJSON order.

Add & list
```bash
# add to an event row (by sequence index)
nexoscli replay annotate add --bag ./bag --target events  --seq 0 \
  --label slow --label investigate --note "p90 high" --author you --confidence 0.8

# add to a file operation row
nexoscli replay annotate add --bag ./bag --target fileops --seq 0 --label write-hot

# bag-level note
nexoscli replay annotate add --bag ./bag --target bag --label reviewed --note "LGTM"

# list (JSON envelope)
nexoscli replay annotate list --bag ./bag --output json
```

Export
```bash
# annotations as CSV (always writes header)
nexoscli replay export --bag ./bag --what annotations

# join annotations into CSVs (adds columns: seq, ann_* )
nexoscli replay export --bag ./bag --what events   --join annotations
nexoscli replay export --bag ./bag --what fileops  --join annotations
```

CSV outputs:
- annotations.csv header:
  ann_ts,source,author,labels,note,confidence,target_kind,target_seq,target_ts,target_run_id,target_plugin,target_command,target_op,target_path,attrs_json
- events_annotated.csv/fileops_annotated.csv: base header + appended:
  seq,ann_labels,ann_note,ann_author,ann_source,ann_confidence,ann_count

Behavior:
- Missing sources still produce CSVs with only headers.
- Joins aggregate per row index (seq). Multiple annotations are combined:
  - labels/authors/sources deduped and `|`-joined
  - notes concatenated with " | "
  - confidence averaged to 3 decimals
  - ann_count reports number of annotations that matched the row
- If no annotations exist, join produces the base CSV (no annotated file), or an annotated file with ann_count=0 depending on availability.

Smoke run
```bash
cargo build --workspace

# Generate a bag
./target/debug/nexoscli dev init --yes
./target/debug/nexoscli term run -- bash -lc 'git init -q && git config user.email t@e && git config user.name T && echo hello> a && git add a && git commit -m init'
RUN=$(tail -n 1 .nexoscli/log/events.jsonl | jq -r '.run_id')
./target/debug/nexoscli replay extract --run-id "$RUN" --out bag

# Annotate
./target/debug/nexoscli replay annotate add --bag bag --target events  --seq 0 --label slow --note "check p90"
./target/debug/nexoscli replay annotate add --bag bag --target fileops --seq 0 --label write-hot

# Export with joins
./target/debug/nexoscli replay export --bag bag --what all --join annotations
ls bag/exports/csv
# annotations.csv events.csv events_annotated.csv fileops.csv fileops_annotated.csv
```
