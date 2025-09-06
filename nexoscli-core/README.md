---
schema: readme.v0
title: "nexoscli-core/"
summary: "Core library providing deterministic providers, events, and plugin contracts."
kind: domain
owner: "@core/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./src/"
in_scope:
  - "Providers, contracts, event logging, registry"
out_of_scope:
  - "Binary composition (see nexoscli-bin)"
inputs:
  - "Workspace configuration and plugin registrations"
outputs:
  - "Library APIs, schemas, and traits for plugins"
tags: ["core","rust"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 0
---
# nexoscli-core — Foundation Library

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated children list from front‑matter or glob._
<!-- END: AUTO -->

## Purpose
Foundational library for deterministic execution, structured observability, and plugin contracts used by the CLI and plugins.

## In-scope
- Providers (Clock, Entropy, IO) and deterministic execution
- Event logging APIs and JSON contracts
- Plugin trait definitions and registry
- Error types and testing utilities

## Out-of-scope
- CLI binary composition and command wiring (see ../nexoscli-bin)
- Domain-specific plugin implementations (see ../plugins)

## Interfaces
- Inputs: workspace configuration, plugin registrations
- Outputs: library APIs, JSON schemas, traits and helpers

Core library providing deterministic, observable infrastructure for nexoscli.
Implements contracts, providers, and plugin architecture.

---

## Architecture

The core library provides the foundational abstractions that enable:
- **Deterministic execution** via injected providers
- **Structured observability** with event logging and tracing
- **Plugin architecture** with trait-based contracts
- **Error handling** with context preservation

```rust
// Core abstractions
pub trait Plugin { /* ... */ }
pub trait ClockProvider { /* ... */ }
pub trait EntropyProvider { /* ... */ }
pub trait IoProvider { /* ... */ }
```

## Modules

### `providers` — Deterministic Dependencies
Abstracts system dependencies for reproducible behavior:

```rust
pub struct Providers {
    pub clock: Arc<dyn ClockProvider>,
    pub entropy: Arc<dyn EntropyProvider>, 
    pub io: Arc<dyn IoProvider>,
}
```

**Key Benefits:**
- No direct `SystemTime::now()` or `thread_rng()` calls
- Testable with mock providers
- Replay-capable execution

### `event` — Structured Event Logging
Append-only event stream for audit and replay:

```rust
pub struct EventLogWriter {
    // Writes to .nexoscli/log/events.jsonl
}

pub struct Event {
    pub ts: DateTime<Utc>,
    pub run_id: String,
    pub plugin: String,
    pub command: String,
    pub args_hash: String,      // SHA256 of CLI args
    pub content_hash: String,   // SHA256 of operation content
    pub exit_code: i32,
    pub attrs: HashMap<String, serde_json::Value>,
}
```

**Features:**
- SHA256 content hashing for integrity
- Automatic span event mirroring
- Structured JSON-per-line format

### `plugin` — Plugin Contracts
Trait definitions for plugin development:

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn commands(&self) -> Vec<clap::Command>;
    fn tool_specs(&self) -> Vec<ToolSpec>;  // AI integration
}

pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub schema: schemars::schema::RootSchema,
    pub examples: Vec<String>,
}
```

**AI Integration:**
- JSON Schema generation for tool introspection
- Standardized tool specifications
- Example-driven agent learning

### `registry` — Plugin Management
Centralized plugin discovery and lifecycle:

```rust
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn register<P: Plugin + 'static>(&mut self, plugin: P);
    pub fn build_cli(&self) -> clap::Command;
    pub fn all_tool_specs(&self) -> Vec<ToolSpec>;
}
```

### `error` — Context-Rich Error Handling
Structured error types with operation context:

```rust
#[derive(Error, Debug)]
pub enum NexoscliError {
    #[error("IO operation failed: {message}")]
    Io { message: String, #[source] source: std::io::Error },
    
    #[error("Plugin error in '{plugin}': {message}")]
    Plugin { plugin: String, message: String },
    
    #[error("Event log error: {message}")]
    EventLog { message: String },
}
```

## Testing Support

### Mock Providers
```rust
pub struct MockClockProvider {
    fixed_time: DateTime<Utc>,
}

pub struct MockEntropyProvider {
    deterministic_seed: u64,
}
```

### Test Utilities
```rust
pub fn test_providers() -> Providers {
    // Returns deterministic providers for testing
}

pub fn test_event_writer() -> EventLogWriter {
    // Returns in-memory event writer for testing
}
```

## Dependencies

| Dependency | Purpose |
|------------|---------|
| `anyhow` | Error context and propagation |
| `thiserror` | Structured error definitions |
| `serde` + `serde_json` | Event serialization |
| `serde_yaml` | Configuration parsing |
| `schemars` | JSON Schema generation |
| `sha2` + `hex` | Content hashing |
| `clap` | Command composition |
| `tracing` | Structured logging |
| `uuid` | Run ID generation |
| `time` | Timestamp handling |
| `dirs` | Standard directory locations |

## Design Principles

1. **Dependency Injection**: All system dependencies are abstracted
2. **Determinism First**: No hidden sources of non-determinism
3. **Observable by Default**: Every operation is logged and traceable
4. **Contract-Driven**: Explicit interfaces over implicit conventions
5. **Test-Friendly**: Mock-able abstractions for reliable testing

## Usage in Plugins

```rust
use nexoscli_core::{Plugin, ToolSpec, Providers, EventLogWriter};

pub struct MyPlugin {
    providers: Arc<Providers>,
    event_writer: Arc<EventLogWriter>,
}

impl Plugin for MyPlugin {
    fn name(&self) -> &'static str { "myplugin" }
    
    fn commands(&self) -> Vec<clap::Command> {
        // Build clap commands
    }
    
    fn tool_specs(&self) -> Vec<ToolSpec> {
        // Export AI tool specifications
    }
}
```

---

For CLI binary implementation details, see [../nexoscli-bin/README.md](../nexoscli-bin/README.md).
For plugin development patterns, see [../plugins/README.md](../plugins/README.md).
