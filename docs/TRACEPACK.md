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

## NEXBags (TracePack v0.2)

NEXBags are portable, analyzable bundles focused on topicized data with a bag-level manifest and integrity verification of the event ledger (event.v0.3). They are analogous to ROS bag files. The bag manifest includes a stable schema tag: "schema": "bag_manifest.v0.2".

Commands:
```bash
# Build a bag (directory or .tgz)
nexoscli replay bag build --run-id RUN-... --out ./tracebag                # dir
nexoscli replay bag build --run-id RUN-... --out ./tracebag.tgz            # tgz

# Verify event hash chain integrity from the bag
nexoscli replay bag verify --bag ./tracebag
nexoscli replay bag verify --bag ./tracebag.tgz

# Show bag summary (counts, topics, versions, time_range)
nexoscli replay bag info --bag ./tracebag

# Cat a topic (bounded by --lines if provided)
nexoscli replay bag cat --bag ./tracebag --topic events
nexoscli replay bag cat --bag ./tracebag --topic events --lines 10
```

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
