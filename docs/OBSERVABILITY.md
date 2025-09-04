# NEXOS Observability

NEXOS provides comprehensive observability through structured event logging, task correlation, and session tracking.

## Event System

All CLI operations emit structured events to `.nexoscli/log/events.jsonl`:

```json
{
  "timestamp": "2025-09-02T12:18:00.123Z",
  "plugin": "task",
  "command": "start",
  "run_id": "RUN-abc123def456",
  "content_hash": "sha256:...",
  "context": {
    "task_id": "TASK-20250902-demo",
    "session_id": "SES-a1b2c3d4e5f6"
  }
}
```

## Sessions

### Session Correlation

Every task operation generates a unique session ID that correlates all related CLI commands and events:

- Format: `SES-` + 12 hex characters (e.g., `SES-a1b2c3d4e5f6`)
- Generation: SHA-256 hash of timestamp + entropy, truncated to 6 bytes
- Persistence: Stored in `.nexoscli/task/current_session`
- Lifecycle: Created on `task start`/`resume`, cleared on `task pause`/`end`

### Session Management

Sessions provide granular tracking within task lifecycles:

```bash
# Start task - generates new session
nexoscli task start --name demo
# All subsequent commands tagged with session_id

# Pause task - clears session
nexoscli task pause
# Commands no longer tagged with task/session context

# Resume task - generates new session
nexoscli task resume --name demo
# New session_id for continued work
```

### Context Injection

While a task is active, all CLI commands automatically include context in:

1. Event logs: `context.task_id` and `context.session_id`
2. JSON envelopes: `meta.context.task_id` and `meta.context.session_id`
3. Task bundles: Session IDs included in manifest

This enables complete traceability of all operations within task boundaries.

## Health Monitoring

### Doctor Checks

The `dev doctor` command provides comprehensive health validation:

```bash
# Run all checks
nexoscli dev doctor

# CI mode with required checks
nexoscli dev doctor --ci --require redaction,events

# Check specific components
nexoscli dev doctor --checks redaction,events,task-coverage
```

#### Available Checks

1. Redaction Config (`redaction`):
   - Validates `.nexoscli/redact.yaml` exists and is well-formed
   - Ensures sensitive data patterns are properly configured

2. Event Integrity (`events`):
   - Validates event log structure and content hashes
   - Checks for corruption or missing events in time windows

3. Task Coverage (`task-coverage`):
   - Measures percentage of events with task context
   - Helps identify workflow adoption and correlation gaps

4. Events Schema (`events-schema`):
   - Parses each events.jsonl line and validates schema tag equals event.v0.3
   - Strengthened structure checks for required fields (ts, plugin, command, exit_code, content_hash); tolerant window via --since

5. Events Chain (`events-chain`):
   - Verifies rolling digest and line count against .nexoscli/log/events.chain.json in CI (read-only)
   - Local runs refresh the chain manifest deterministically

#### CI Integration

Use `--require` flag to enforce specific checks in CI pipelines:

```bash
nexoscli dev doctor --ci --require redaction,events
# Exit code 1 if any required check fails
```

## TracePack Integration

Task bundles include complete observability data:

```bash
nexoscli task bundle --task TASK-20250902-demo --out bundle.tgz
```

Bundle contents:
- `events.jsonl` - All task-related events
- `manifest.json` - Task metadata including session IDs
- `status.json` - Final task state
- `patches/` - Code changes and diffs
- `cline/` - Cline conversation history (if available)

## NEXBags (TracePack v0.2)

Use NEXBags for portable, analyzable capture of runs with event-ledger integrity.

Commands:
```bash
# Build bag (dir or .tgz)
nexoscli replay bag build --run-id RUN-... --out ./tracebag
nexoscli replay bag build --run-id RUN-... --out ./tracebag.tgz

# Verify event ledger integrity (event.v0.3 content/chain hash)
nexoscli replay bag verify --bag ./tracebag
nexoscli replay bag verify --bag ./tracebag.tgz

# Summarize contents and topics
nexoscli replay bag info --bag ./tracebag

# Stream a topic (bounded)
nexoscli replay bag cat --bag ./tracebag --topic events --lines 10
```

Notes:
- Integrity is computed over the canonical subset {plugin, command, exit_code, dur_ms, data} for content_hash and chained via chain_hash(prev+content_hash).
- See docs/TRACEPACK.md for bag layout and schemas (bag_manifest_v0.2, term_record_v0.1, file_watch_event_v0.1).

## Best Practices

### Task Hygiene
- Always `task start` before beginning work
- Use `task pause` when switching contexts
- Bundle completed tasks for archival: `task bundle`

### Event Correlation
- Session IDs enable fine-grained work correlation
- Task IDs provide higher-level project correlation
- Run IDs trace individual CLI command execution

### Health Monitoring
- Run `dev doctor` regularly to validate system health
- Include doctor checks in CI/CD pipelines
- Monitor task coverage metrics for workflow adoption

### Redaction
- Configure `.nexoscli/redact.yaml` for sensitive data
- Test redaction patterns with sample data
- Review event logs for accidental exposure

## Event v0.3 (Tamper‑evident Ledger)

NEXOS augments each event with a hash chain for tamper evidence when the ledger is enabled (default; set NEXOS_LEDGER=0 to disable during rollout).

Example (abridged):
```json
{
  "schema": "event.v0.3",
  "ts": "2025-09-02T12:18:00.123Z",
  "run_id": "RUN-abc123def456",
  "seq": 2,
  "prev": "sha256:1a2b...c3d4",
  "content_hash": "sha256:9f8e...7a6b",
  "chain_hash": "sha256:0abc...def1",
  "plugin": "task",
  "command": "start",
  "exit_code": 0,
  "dur_ms": 25,
  "data": { "...": "..." },
  "context": { "task_id": "TASK-...", "session_id": "SES-..." }
}
```

- content_hash = sha256(canonical subset)
- chain_hash = sha256(prev + content_hash)
- seq starts at 1 and increments per event in a run

Per‑run files and manifest:
- .nexoscli/log/runs/<RUN-ID>.jsonl (per‑run events)
- .nexoscli/log/runs/<RUN-ID>.manifest.json containing:
  { "version":"0.2","first_seq","last_seq","first_hash","last_hash","count","time_range","chain_ok":true }

## Schema Registry and Validation

All emitted records carry a stable "schema" tag (e.g., "event.v0.3", "bag_manifest.v0.2"). Schemas are derived from the real Rust types (single source of truth) and can be exported and validated for drift.

Export contract schemas:
```bash
./target/debug/nexoscli dev schemas export --out docs/SPECS
```

CI drift check (fail if schemas changed):
```bash
git diff --exit-code -- docs/SPECS
```

See docs/SPECS/README.md for details and do-not-hand-edit guidance.

### Chain-of-custody (events)

NEXOS maintains a rolling digest over `.nexoscli/log/events.jsonl`:

H_0 = sha256("")

H_n = sha256(H_{n-1} || "\n" || raw_line_n)

The doctor writes/validates a manifest at `.nexoscli/log/events.chain.json`:

```json
{
  "schema": "events_chain.v0.1",
  "file": ".nexoscli/log/events.jsonl",
  "from_ts": "2025-09-02T07:29:15Z",
  "to_ts": "2025-09-02T07:29:45Z",
  "lines": 42,
  "sha256": "sha256:…",
  "generated_at": "2025-09-02T07:30:00Z"
}
```

- Local runs refresh the file:

```
nexoscli dev doctor --checks events-schema,events-chain
```

- CI verifies (no writes) and can enforce:

```
nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain
