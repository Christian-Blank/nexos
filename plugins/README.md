---
schema: readme.v0
title: "Plugins/"
summary: "Extensible CLI functionality grouped by plugin crates."
kind: domain
owner: "@cli/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "../nexoscli-bin/Cargo.toml"
  - "./*/Cargo.toml"
in_scope:
  - "Plugin crates and their READMEs"
out_of_scope:
  - "Core contract types (see nexoscli-core)"
inputs:
  - "Workspace Cargo configuration"
outputs:
  - "Registered commands in nexoscli-bin"
tags: ["plugins","rust"]
auto:
  children_glob: ["*/README.md"]
  dir_tree_depth: 1
---
# Plugins — Domain Modules

<!-- BEGIN: AUTO (id=children) -->
_Auto-generated: list of plugin READMEs with short descriptions._
<!-- END: AUTO -->

Static plugin architecture for nexoscli domains.
Each plugin extends the CLI with domain-specific commands while maintaining strict contracts.

---

## Architecture Overview

Plugins are **statically linked** domain modules that extend the core CLI:
- Single binary with all plugins compiled in
- Feature-flagged activation (no runtime discovery)
- Trait-based contracts for consistent behavior
- JSON Schema introspection for AI tooling

```rust
pub trait Plugin {
    fn name(&self) -> &'static str;
    fn commands(&self) -> Vec<Command>;
    fn tool_specs(&self) -> Vec<ToolSpec>;  // For AI integration
}
```

## Plugin Contracts

### Command Integration
- Commands are composed into a single clap tree at startup
- Each plugin contributes its subcommands to the global CLI
- Help, completions, and documentation are automatically generated

### JSON Output Contract
All plugin commands must support `--output json` with this structure:
```json
{
  "data": { /* plugin-specific result */ },
  "errors": [],
  "meta": { "run_id": "uuid" }
}
```

### Event Logging
Commands automatically emit structured events to `.nexoscli/log/events.jsonl`:
```json
{
  "ts": "2025-09-01T12:34:56Z",
  "run_id": "uuid",
  "plugin": "plugin_name",
  "command": "subcommand",
  "args_hash": "sha256:...",
  "content_hash": "sha256:...",
  "exit_code": 0,
  "attrs": { /* plugin-specific metadata */ }
}
```

## Development Pattern

### 1. Plugin Structure
```
plugins/
├── README.md              # This file (common architecture)
├── my_plugin/
│   ├── README.md          # Plugin-specific docs
│   ├── Cargo.toml         # Plugin dependencies
│   └── src/
│       └── lib.rs         # Plugin implementation
```

### 2. Implementation Steps
1. **Define the plugin struct** implementing the `Plugin` trait
2. **Create command handlers** with proper error handling
3. **Implement JSON Schema** for AI tool integration via `ToolSpec`
4. **Add to workspace** in root `Cargo.toml`
5. **Register in core** plugin registry

### 3. Testing Strategy
- Unit tests for command logic
- Integration tests in `nexoscli-bin/tests/`
- JSON contract validation
- Event log verification

## AI Integration

Plugins expose their capabilities through `ToolSpec` for AI agents:
- **JSON Schema** for input validation
- **Tool descriptions** for agent planning  
- **Example usage** for few-shot learning

```rust
impl ToolSpec for MyPlugin {
    fn tool_specs(&self) -> Vec<nexoscli_core::plugin::ToolSpec> {
        vec![ToolSpec {
            name: "my_command".to_string(),
            description: "Brief description for AI".to_string(),
            schema: my_command_schema(), // JSON Schema
            examples: vec!["nexoscli my_plugin my_command --arg value"],
        }]
    }
}
```

## Current Plugins

| Plugin | Status | Domain |
|--------|--------|---------|
| `issue` | ✅ Active | Local issue tracking |
| `agents` | 🚧 Future | AI agent workflows |
| `connectors` | 🚧 Future | External system adapters |

## Feature Gates

Plugins can be conditionally compiled:
```toml
[features]
default = ["plugin-issue"]
plugin-issue = ["plugins/issue"]
plugin-agents = ["plugins/agents"]  # future
```

## Observability

### Tracing
Plugins inherit automatic tracing spans:
- Command execution spans
- Input/output logging
- Error propagation
- Performance metrics

### Event Stream
The event log provides:
- **Audit trail** of all plugin operations
- **Replay capability** for debugging
- **Analytics data** for usage patterns
- **Integration points** for external monitoring

---

## Design Principles

1. **Static over Dynamic**: Compile-time composition for reliability
2. **Contracts over Conventions**: Explicit interfaces and schemas
3. **Observability First**: Every operation is logged and traceable
4. **AI-Native**: Built for human-AI collaboration from day one

For plugin-specific details, see individual plugin README files.
