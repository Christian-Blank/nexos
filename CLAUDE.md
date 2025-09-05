# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

NEXOS is a plugin-based CLI framework written in Rust that emphasizes deterministic execution, event sourcing, and machine-readable interfaces. It's designed for human-AI collaboration with strict contracts and comprehensive observability.

## Build and Development Commands

### Core Commands
```bash
# Build the entire workspace
cargo build --workspace

# Run all tests
cargo test --workspace

# Run a single test
cargo test --workspace test_name

# Run the CLI
./target/debug/nexoscli --help

# Machine-readable output (always use for parsing)
./target/debug/nexoscli --output json <command>

# Comprehensive health check (run after changes)
./target/debug/nexoscli dev doctor --ci --require redaction,events

# Initialize project structure
./target/debug/nexoscli dev init --yes
```

### Development with Observability
```bash
# Build with OpenTelemetry support
cargo build --workspace --features otel

# Run with tracing enabled
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317 RUST_LOG=info ./target/debug/nexoscli --features otel <command>
```

## Architecture and Code Structure

### Plugin-Based Architecture
- **Static Plugins**: All plugins are compiled into a single `nexoscli` binary (no dynamic loading)
- **Plugin Location**: `/plugins/` directory contains domain-specific functionality
- **Core Library**: `nexoscli-core/` provides shared types, providers, and plugin system
- **Main Binary**: `nexoscli-bin/` composes plugins into the CLI command tree

### Key Architectural Patterns
1. **Deterministic Execution**: Use injected providers (`Clock`, `Entropy`, `FilesystemAdapter`) - never use `SystemTime::now()` or `thread_rng()` directly
2. **Event Sourcing**: All operations append to `.nexoscli/log/events.jsonl` with content hashing
3. **JSON Contracts**: Use `--output json` for machine interfaces, returns `{ data, errors, meta }` envelope
4. **Command Context**: Every plugin receives `CommandContext` with deterministic providers
5. **Task Context Injection**: Active tasks (`.nexoscli/task/ACTIVE`) automatically tag all commands

### Plugin Development
When creating or modifying plugins:
1. Implement the `Plugin` trait with `execute()` method
2. Use `CommandContext` providers for all non-deterministic operations
3. Return structured data that serializes to JSON
4. Add tracing spans for observability
5. Follow the existing plugin structure in `/plugins/`

### Event System
- Events are logged to `.nexoscli/log/events.jsonl`
- Each event has: timestamp, correlation_id, span_id, command, data, content hash
- Use `nexoscli replay` commands to analyze event logs
- TracePacks bundle events with referenced files for reproduction

### Testing Approach
- Unit tests in each module (run with `cargo test`)
- Integration tests use deterministic providers for reproducibility
- Health checks validate system invariants (`dev doctor`)
- Event log integrity checked automatically

## Critical Design Principles

1. **Contracts Over Conventions**: Everything has stable JSON schemas (see `docs/SPECS/`)
2. **Tighten-Only Contexts**: Child contexts can narrow constraints but not widen them
3. **Observable by Default**: All operations emit structured traces and events
4. **Machine-First Interfaces**: JSON output is the primary contract, human output is secondary
5. **Content-Addressed Storage**: Files referenced by SHA256 hash in events

## Common Development Tasks

### Adding a New Plugin
1. Create directory under `/plugins/`
2. Add to workspace in root `Cargo.toml`
3. Implement `Plugin` trait
4. Register in `nexoscli-bin/src/main.rs`
5. Add tests and update health checks

### Modifying Event Schema
1. Update schema version in `nexoscli-core/src/events/`
2. Add migration logic if needed
3. Update `docs/SPECS/` documentation
4. Run `nexoscli tools export` to regenerate schemas

### Working with Tasks
- Tasks provide context injection for all commands
- Active task stored in `.nexoscli/task/ACTIVE`
- Use `nexoscli task` commands to manage lifecycle
- Tasks automatically tag events and file operations

## Project-Specific Conventions

- **Error Handling**: Use `anyhow` for application errors, `thiserror` for library errors
- **Async Code**: Tokio runtime for async operations
- **Serialization**: Serde with `#[serde(rename_all = "snake_case")]` for JSON
- **File Paths**: Always use `PathBuf` and handle cross-platform differences
- **Timestamps**: ISO 8601 format with timezone (use `Clock` provider)
- **Hashing**: SHA256 for content addressing (use `HashingAdapter` provider)