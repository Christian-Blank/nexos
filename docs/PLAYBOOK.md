# Engineering Playbook (v0)

## Working Agreements

- **WIP**: 1 feature in progress, 1 meta change in review.
- **Rule of 3**: third repeat → automate (generator); risky multi-file edits → automate now.
- **Tighten-only**: children may narrow budgets/constraints; widening requires CP→ADR.
- **Small PRs**: ≤ ~300 LOC or ≤ 6 files.
- **Default to action**: stuck > 10 min → open an atomic_issue and move.

## Double Learning Loop (after each milestone)

**L1 — Feature Review (≤15 min):** value shipped, evidence (spans/events/demo), 3 frictions → issues, pick next feature.  
**L2 — Meta Review (≤10 min, only if triggered):** CP if (1) blockage, (2) repeats across ≥2 features, or (3) violates a budget. If accepted → ADR.

## Definitions

**Ready:** problem ≤ 3 sentences; DRI named; deps listed; ≤ 8 binary acceptance criteria; added to exec_graph.  
**Done:** E2E passes; JSON outputs stable; event written; spans present; brief README snippet updated if surface changed; short demo/evidence linked.


## CLI Development

**Build, test, and run** the NEXOS CLI:

```bash
# Build the workspace
cargo build --workspace

# Run tests
cargo test --workspace

# Run CLI commands
./target/debug/nexoscli --help
./target/debug/nexoscli issue new --title "example"
./target/debug/nexoscli --output json issue list

# Check event log
cat .nexoscli/log/events.jsonl
```

**Development cycle:**
1. Make changes to core or plugins
2. Run `cargo build --workspace` to compile
3. Test with `cargo test --workspace`
4. Verify CLI functionality manually
5. Check event logs for proper instrumentation

## New Project Setup

**Initialize a new project** with dev ergonomics:

```bash
# Plan what will be created (dry-run)
./target/debug/nexoscli dev init --dry-run --output json

# Apply initialization (idempotent)
./target/debug/nexoscli dev init --yes --output json

# Manual init (prompts for confirmation)
./target/debug/nexoscli dev init
```

This creates `.nexoscli/` directory, `manifest.toml`, and updates `.gitignore`.

## Issue Import

**Import issues from external sources**:

```bash
# Create a JSON file with issues
echo '[{"title":"Bug fix","labels":["p0","bug"],"body":"Details here"}]' > issues.json

# Import from file (auto-increment IDs)
./target/debug/nexoscli issue import --from file --path issues.json --output json

# Import preserving existing IDs
./target/debug/nexoscli issue import --from file --path issues.json --id-strategy keep --output json

# Verify import worked
./target/debug/nexoscli issue list --output json
```

Expected JSON format: `[{"title": "required", "body": "optional", "labels": ["array","or","string,separated"]}]`

## Task Management

**Task lifecycle management** with automatic context injection:

```bash
# Create a new task
./target/debug/nexoscli task new --title "Implement feature X"

# Start working on a task (makes it active)
./target/debug/nexoscli task start --task TASK-20250902-implement-feature-x

# Any command now gets automatic context injection
./target/debug/nexoscli issue new --title "Add unit tests"
# Returns with context: {"task_id": "TASK-...", "stage": "planning"}

# Change task stage
./target/debug/nexoscli task stage implementation

# Check task status with session history and event counts
./target/debug/nexoscli task status --output json

# Pause task when switching context
./target/debug/nexoscli task pause

# Resume a paused task
./target/debug/nexoscli task resume --task TASK-20250902-implement-feature-x

# List all tasks
./target/debug/nexoscli task list --output json

# End the task when complete
./target/debug/nexoscli task end

# Create a TracePack bundle with all task-related artifacts
./target/debug/nexoscli task bundle --task TASK-20250902-implement-feature-x --out ./bundle.tgz
```

**Engine selection:** Choose execution engine for task transitions:

```bash
# Default local engine (runs hooks/gates immediately)
./target/debug/nexoscli task start --task TASK-20250902-example
./target/debug/nexoscli task stage implementation

# Temporal engine (writes envelopes to outbox for later processing)
./target/debug/nexoscli task start --task TASK-20250902-example --engine temporal
./target/debug/nexoscli task stage implementation --engine temporal
./target/debug/nexoscli task end --engine temporal

# Inspect temporal envelopes before sending
ls .nexoscli/engine/outbox/
cat .nexoscli/engine/outbox/*.json
```

**When to use temporal engine:** If you intend to hand off to Temporal later, use `--engine temporal` to produce outbox envelopes you can inspect or feed to a client.

**Task stages:** planning → implementation → verification → done (or blocked)

**Context injection:** When a task is active, ALL commands automatically get tagged with `{task_id, stage}` in events and JSON envelopes - no plugin changes required!

## Plan Lifecycle (CLI-native)

Manage plans deterministically with the dev plugin — do not hand-edit plan front‑matter or move plan files manually.

Quick start:
```
nexoscli dev init --yes
nexoscli dev plans start --output json
# implement work…
nexoscli dev plans finalize --output json
```

What happens:
- Start: creates + starts a task, updates front‑matter (status=in-progress, plan_id, slug, task_id, started_at, source=next), moves next → current
- Finalize: records implemented_in (HEAD), writes completed_at + status=done, ends task + bundle, moves current → done

Troubleshooting:
- If an active task blocks start:
  - Pause: `nexoscli task pause`
  - Or end: `nexoscli task end --task <ID> || true`
  - Retry start: `nexoscli dev plans start --output json`

References:
- docs/project/README.md — plan folders and commands
- docs/prompts/cline/start_new_task.workflow.md
- docs/prompts/cline/finalize_task.workflow.md

## Health Checks

**System health validation** with comprehensive doctor checks:

```bash
# Run all available health checks
./target/debug/nexoscli dev doctor --output json

# CI mode - exit code 1 if any required checks fail
./target/debug/nexoscli dev doctor --ci --require redaction,events

# Run specific checks only
./target/debug/nexoscli dev doctor --checks redaction,events,task-coverage

# Check event integrity for specific time window
./target/debug/nexoscli dev doctor --since 24h --checks events
```

**Available checks:**
- **redaction**: Validates `.nexoscli/redact.yaml` exists and is well-formed
- **events**: Validates event log structure and content integrity
- **task-coverage**: Measures percentage of events with task context (helps track workflow adoption)
- **events-schema**: Parses `.nexoscli/log/events.jsonl` lines, validates schema tag equals `event.v0.3`, and enforces required fields (ts, plugin, command, exit_code, content_hash); honors `--since`
- **events-chain**: Verifies rolling digest and line count against `.nexoscli/log/events.chain.json` in CI (read-only); local runs refresh deterministically
- **events-rotation**: Verifies `.nexoscli/log/events.index.json` continuity by recomputing each segment sha256_file/lines and rolling head; fails on any mismatch (CI-read-only)

**CI Integration:**
```bash
# In CI pipeline - enforce critical checks
./target/debug/nexoscli dev doctor --ci --require redaction,events
# Script exits with code 1 if any required check fails

# Development workflow - run before commits
./target/debug/nexoscli dev doctor --checks redaction,events,task-coverage
```

# Event integrity + chain-of-custody (recommended for CI)
```bash
nexoscli dev doctor --ci --checks events-schema,events-chain --require events-schema,events-chain
```

# Rotate the event log periodically (local/dev)
Keep `.nexoscli/log/events.jsonl` small and evidence durable.

```bash
# Rotate by size threshold and/or once per day
nexoscli dev log rotate --max-bytes 5000000 --daily

# Force rotation even if thresholds not met
nexoscli dev log rotate --force

# Inspect rotation/index status
nexoscli dev log status --output json
```

# Verify rotation continuity in CI
```bash
nexoscli dev doctor --ci --checks events-rotation --require events-rotation
```

**Health monitoring best practices:**
- Run `dev doctor` regularly during development
- Include doctor checks in CI/CD pipelines  
- Monitor task coverage metrics to track workflow adoption
- Use `--require` flag to enforce critical checks in automated environments
- Cline correlation coverage (info): `nexoscli dev doctor --ci --checks cline` runs `cline locate` and a dry‑run `cline import`, then computes task coverage and message_coverage via `cline correlate --since 24h --pad 5m --dry-run` and reports both in the cline check detail. Ensure host globalStorage is mounted or set `NEXOS_CLINE_STORAGE` in Dev Containers/CI.

### Doctor v0 profile

One‑flag CI preset with sensible defaults for v0. Required gates: `graph_check`, `plugins_check`, `event_log_writable`. Cline coverage is informational (soft).

```bash
# Default preset
nexoscli dev doctor --ci --profile v0

# Optional: explicit checks and required gates
nexoscli dev doctor --ci --profile v0 \
  --checks "graph,plugins,config,events,tools,cline,rules" \
  --require "graph_check,plugins_check,event_log_writable"
```

## TracePack & NEXBags (TracePack v0.2)

Portable run artifacts for analysis, sharing, and replay.

- TracePack (legacy extractor): directory/tgz with manifest, events, patches, status.
- NEXBags (v0.2): topicized bag with bag-level manifest and inline integrity verification (event.v0.3 hash chain). See docs/TRACEPACK.md.

Common flows:
```bash
# Build legacy TracePack for a run
./target/debug/nexoscli replay extract --run-id RUN-... --out .nexoscli/tracepacks/pack-1

# Build a NEXBag (dir or .tgz)
./target/debug/nexoscli replay bag build --run-id RUN-... --out ./tracebag
./target/debug/nexoscli replay bag build --run-id RUN-... --out ./tracebag.tgz

# Verify bag integrity (events stats + hash-chain)
./target/debug/nexoscli replay bag verify --bag ./tracebag
./target/debug/nexoscli replay bag verify --bag ./tracebag.tgz
# Strict mode: exact time_range equality; require digests when present
./target/debug/nexoscli replay bag verify --bag ./tracebag --strict
# Cross-check against rotation index (Plan-29 segment-chain)
./target/debug/nexoscli replay bag verify --bag ./tracebag --index .nexoscli/log/events.index.json

# Inspect bag metadata and topics
./target/debug/nexoscli replay bag info --bag ./tracebag

# Stream a topic from the bag
./target/debug/nexoscli replay bag cat --bag ./tracebag --topic events --lines 10
```

Notes:
- Integrity: bag.build and bag.verify recompute content_hash over the canonical subset {plugin, command, exit_code, dur_ms, data} and chain_hash(prev+content_hash).
- Cline tasks: bag.build may include normalized tasks by time window (±5m) if present under .nexoscli/cline/tasks/.
- Schemas: bag manifest v0.2 (docs/SPECS/bag_manifest_v0.2.schema.json), terminal record v0.1 (reserved), file-watch event v0.1 (optional).
- Reference: docs/TRACEPACK.md.

## OTEL Development

**Observability stack** for development with OpenTelemetry:

```bash
# Dev container (recommended)
# Open in VS Code -> Dev Containers: Reopen in Container
# Automatically starts OTel collector + Jaeger

# Build with OTEL support
cargo build --workspace --features otel

# Run with observability (spans → Jaeger UI)
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 \
RUST_LOG=info \
./target/debug/nexoscli --features otel issue new --title "traced command"

# View spans: http://localhost:16686 (Jaeger UI)
# Service: nexoscli, spans: run → execute with plugin context
```

**Tools & Schema Export:**
```bash
# Export JSON schemas for all commands
./target/debug/nexoscli tools export

# Generate shell completions
./target/debug/nexoscli completions bash > nexoscli.bash

# Generate man page  
./target/debug/nexoscli man
```

Schema registry (contracts) export and drift check:
```bash
# Export contract schemas from real types (schemars)
./target/debug/nexoscli dev schemas export --out docs/SPECS

# CI drift check (fail if schemas changed)
git diff --exit-code -- docs/SPECS
```

**Event Log Structure:** `.nexoscli/log/events.jsonl` with content hashing for integrity.

## Folder Rule

Any bounded context (folder) must have a one-page README with project_brief front-matter and a link to its parent. Child NS optional; if present, it only tightens budgets.

## Workflow Plugin (Phase 18)

Brief: The workflow plugin adds signals, queries, visibility, and Temporal-style timeouts/retries.

Examples:
```bash
./target/debug/nexoscli workflow init --id WF-demo
./target/debug/nexoscli workflow plan --spec .nexoscli/workflows/WF-demo/spec.yaml --output json
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --output json
./target/debug/nexoscli workflow status --id WF-demo --output json

# Signals and queries
./target/debug/nexoscli workflow signal --id WF-demo --name pause
./target/debug/nexoscli workflow signal --id WF-demo --name resume
./target/debug/nexoscli workflow signal --id WF-demo --name stop
./target/debug/nexoscli workflow query  --id WF-demo --name status --output json

# Visibility list and upsert of search attributes
./target/debug/nexoscli workflow list --filter 'task_queue=nexos' --output json
./target/debug/nexoscli workflow upsert-sa --id WF-demo --attrs owner=me --output json
```

Spec additions and operational details: see docs/WORKFLOWS_PLUGIN.md.

### Timer step (Phase 19) quick example

Use a durable timer step that persists its due time and honors pause/resume/stop:

```bash
# Demo spec
./target/debug/nexoscli workflow plan --spec docs/workflows/WF-timer-demo.yaml --output json
time ./target/debug/nexoscli workflow run  --spec docs/workflows/WF-timer-demo.yaml --output json

# Re-run (idempotent timer step may skip if prior effect ok)
./target/debug/nexoscli workflow run --spec docs/workflows/WF-timer-demo.yaml --output json

# Pause/resume a longer timer
./target/debug/nexoscli workflow init --id WF-slow
# edit .nexoscli/workflows/WF-slow/spec.yaml -> set: steps: [{ id: t1, timer: { duration_ms: 5000 }, idempotent: true }]
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-slow/spec.yaml --output json &
sleep 1
./target/debug/nexoscli workflow signal --id WF-slow --name pause
sleep 2
./target/debug/nexoscli workflow signal --id WF-slow --name resume
wait
```

Behavior:
- Durable: due time saved at .nexoscli/workflows/<id>/timers/<step>.json
- Signals: pause blocks until resume; stop cancels the step and run
- Heartbeats: updated while waiting if heartbeat_ms is set
- Timeouts: start_to_close per attempt; schedule_to_close across attempts (with preflight guard)
- Idempotent: same input → skip on re-run if prior effect was ok
