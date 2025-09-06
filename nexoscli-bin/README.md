---
schema: readme.v0
title: "nexoscli-bin/"
summary: "Main CLI binary composing core, plugins, and telemetry into nexoscli."
kind: domain
owner: "@cli/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./src/"
in_scope:
  - "Binary composition, telemetry, and command wiring"
out_of_scope:
  - "Core providers/contracts (see nexoscli-core)"
inputs:
  - "Core library and plugin crates"
outputs:
  - "nexoscli binary with composed command tree"
tags: ["cli","rust"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 0
---
# nexoscli-bin — CLI Binary

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated children list from front‑matter or glob._
<!-- END: AUTO -->

Main CLI binary that composes core library, plugins, and telemetry into the nexoscli command.
Implements the walking skeleton with structured observability.

---

## Architecture

The binary serves as the composition root that wires together:
- **Core infrastructure** from `nexoscli-core`
- **Domain plugins** from `plugins/*` 
- **Telemetry stack** (optional OTEL integration)
- **Command composition** via clap

```rust
fn main() {
    let providers = Providers::default();
    let event_writer = EventLogWriter::new(&providers);
    let registry = build_plugin_registry();
    let cli = registry.build_cli();
    
    // Execute with full observability
    run_with_telemetry(cli, providers, event_writer).await
}
```

## Modules

### `main.rs` — Application Entry Point
Core responsibilities:
- **Argument parsing** and command dispatch
- **Provider initialization** (Clock, Entropy, IO)
- **Plugin registry** composition
- **Telemetry lifecycle** management
- **Event logging** coordination
- **Span lifecycle** management

**Key Features:**
```rust
// JSON contract enforcement
if args.output == OutputFormat::Json {
    let result = CommandResult { data, errors, meta };
    println!("{}", serde_json::to_string_pretty(&result)?);
}

// Automatic event logging
event_writer.write_event(Event {
    plugin: "issue",
    command: "new", 
    args_hash: hash_args(&args),
    content_hash: hash_content(&result),
    // ...
}).await?;
```

### `telemetry.rs` — OpenTelemetry Integration
Feature-gated OTEL support for distributed tracing:

```rust
#[cfg(feature = "otel")]
pub async fn init_telemetry() -> Result<TracingGuard> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;
        
    tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();
        
    Ok(TracingGuard::new())
}
```

**Integration Points:**
- **Jaeger UI**: Visual trace inspection at http://localhost:16686
- **OTLP Export**: Standard OpenTelemetry protocol
- **Span Events**: Automatic event log mirroring
- **Graceful Shutdown**: Proper trace flushing

## Features

### Default Build
```toml
[features]
default = []
```
Minimal build without OTEL dependencies:
- Core functionality only
- Local tracing via `tracing-subscriber`
- Event logging to `.nexoscli/log/events.jsonl`

### OTEL Build
```toml
[features]
otel = ["opentelemetry", "opentelemetry_sdk", "opentelemetry-otlp", "tracing-opentelemetry", "tokio"]
```
Full observability stack:
- OpenTelemetry tracing
- OTLP export to collectors
- Distributed trace correlation
- Async runtime support

## Commands

### Core CLI Structure
```
nexoscli
├── issue                    # Issue tracking plugin
│   ├── new                 # Create issue
│   ├── list                # List issues  
│   ├── show                # Show issue details
│   └── close               # Close issue
├── tools                   # AI integration
│   ├── export             # Export tool schemas
│   └── validate           # Validate tool usage
└── completions            # Shell completions
    ├── bash
    ├── fish
    └── zsh
```

### Global Options
```bash
--output json              # JSON output format
--log-level debug         # Tracing verbosity
--run-id <uuid>           # Override run ID
```

### Tool Export for AI
```bash
nexoscli tools export --format json > tools.json
```
Exports comprehensive tool specifications for AI agents:
```json
{
  "tools": [
    {
      "name": "issue_new",
      "description": "Create a new issue",
      "schema": { /* JSON Schema */ },
      "examples": ["nexoscli issue new --title 'Bug fix' --kind bug"]
    }
  ]
}
```

## Dependencies

### Core Runtime
- `nexoscli-core`: Foundation library
- `clap`: Command-line parsing
- `serde_json` + `serde_yaml`: Serialization
- `tracing`: Structured logging
- `uuid`: Run ID generation

### Plugin Integration
- `nexoscli-plugin-issue`: Issue tracking plugin
- Future plugins loaded via feature flags

### Optional OTEL Stack
- `opentelemetry` + `opentelemetry_sdk`: Core OTEL
- `opentelemetry-otlp`: OTLP export protocol
- `tracing-opentelemetry`: Tracing bridge
- `tokio`: Async runtime for OTEL

### Development/Testing
- `assert_cmd`: CLI testing framework
- `predicates`: Output assertions
- `tempfile`: Temporary directories for tests

## Building

### Standard Build
```bash
cargo build --release
```
Produces minimal binary without OTEL dependencies.

### OTEL Build
```bash
cargo build --release --features otel
```
Full observability build with distributed tracing.

### Dev Container
```bash
# In .devcontainer environment
cargo build --features otel
./target/debug/nexoscli --help
```

## Testing

### Integration Tests
Located in `tests/` directory:
- **CLI contract validation**: JSON output format
- **Event log verification**: Proper event structure
- **Plugin integration**: End-to-end command flows
- **Error handling**: Graceful failure scenarios

```rust
#[test]
fn test_issue_flow() {
    let mut cmd = Command::cargo_bin("nexoscli")?;
    let output = cmd
        .args(&["issue", "new", "--title", "Test", "--kind", "bug"])
        .assert()
        .success()
        .get_output();
        
    // Verify JSON contract
    let result: CommandResult = serde_json::from_slice(&output.stdout)?;
    assert!(result.errors.is_empty());
    assert!(result.data["id"].as_str().unwrap().starts_with("ISS-"));
}
```

### Performance Testing
- Startup time measurement
- Memory usage profiling  
- Event log throughput
- OTEL export latency

## Observability

### Local Tracing
Default structured logging to stdout:
```
2025-09-01T12:34:56Z  INFO nexoscli: Starting command
2025-09-01T12:34:56Z DEBUG nexoscli::issue: Creating issue ISS-0001
2025-09-01T12:34:56Z  INFO nexoscli: Command completed successfully
```

### Distributed Tracing (OTEL)
When built with `--features otel`:
- **Jaeger UI**: http://localhost:16686
- **Trace context**: Automatic parent-child relationships
- **Span attributes**: Rich metadata for debugging
- **Event correlation**: Event log entries include trace IDs

### Event Stream
All operations logged to `.nexoscli/log/events.jsonl`:
```json
{"ts":"2025-09-01T12:34:56Z","run_id":"...","plugin":"issue","command":"new","exit_code":0}
```

---

## Quick Start

```bash
# Build and install
cargo install --path .

# Basic usage
nexoscli issue new --title "My first issue" --kind feature
nexoscli issue list --output json

# Enable tracing (with OTEL feature)
RUST_LOG=debug nexoscli issue new --title "Traced issue" --kind bug

# Export tools for AI
nexoscli tools export > ai_tools.json
```

For core library details, see [../nexoscli-core/README.md](../nexoscli-core/README.md).
For plugin development, see [../plugins/README.md](../plugins/README.md).
