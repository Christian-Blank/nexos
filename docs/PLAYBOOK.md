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

**Event Log Structure:** `.nexoscli/log/events.jsonl` with content hashing for integrity.

## Folder Rule

Any bounded context (folder) must have a one-page README with project_brief front-matter and a link to its parent. Child NS optional; if present, it only tightens budgets.
