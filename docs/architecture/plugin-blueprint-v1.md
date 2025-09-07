---
schema: "doc.v0"
title: "Plugin Blueprint v1"
owner: "@cli/dri"
status: "live"
version: "1.0.0"
last_reviewed: "2025-09-07T00:00:00Z"
parents: ["../README.md","../../README.md"]
tags: ["architecture","plugins","refactor","agents"]
---

# Plugin Blueprint v1

This document is the canonical specification for NexOS CLI plugin crates.

## 1. Folder layout

plugins//
README.md
Cargo.toml         # name = “nexoscli-plugin-”
src/
lib.rs           # thin router; imports cmd_*; defines Plugin impl & tool_specs()
cmd_.rs   # subcommand module (args/result, run(), subcommand())
tests/
smoke.rs         # at least one co-located test; E2E remains in nexoscli-bin/tests/

## 2. Contracts
- JSON envelopes via `ctx.ok/ctx.err`.
- Tool specs: each command publishes `ToolSpec` with `args_schema` and `result_schema` (schemars).
- Args/result structs derive: `Serialize`, `Deserialize`, `JsonSchema` (optionally `clap::Args` later).
- Events: every run writes to `.nexoscli/log/events.jsonl` (core behavior).
- Docs: README front‑matter + `AUTO (id=commands)` and `AUTO (id=tools)` sections.

## 3. Naming & flags
- Crate: `nexoscli-plugin-<name>`, module files `cmd_<topic>.rs`.
- Long flags are `--kebab-case`. Reserve short flags for strongly standard cases.

## 4. Module pattern
Each `cmd_*.rs` exports:
```rust
pub fn subcommand() -> clap::Command { /* adds about/args */ }
pub fn run(m: &clap::ArgMatches, ctx: &CommandContext) -> Result<i32> { /* ctx.ok/ctx.err */ }
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct <Cmd>Args { /* typed args for schema tooling */ }
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
pub struct <Cmd>Result { /* typed result for schema tooling */ }
```

## 5. tool_specs()
- Provide one ToolSpec per command with concise about, args_schema, and result_schema.

## 6. Testing
- Co-locate minimal tests per plugin under tests/ (unit/integration).
- Keep E2E (binary-level) in `nexoscli-bin/tests/`.

## 7. Migration playbook (per plugin)
1. Create `tests/smoke.rs`.
2. Split the first subcommand into `src/cmd_<topic>.rs` with `subcommand()` + `run()`.
3. Wire `lib.rs` to call the module; keep behavior identical.
4. Add typed args/result structs; wire `ToolSpec`.
5. Repeat per subcommand; keep files < ~800 LOC.
6. Run `nexoscli dev plugins check` and CI doctor gates.

## 8. Enforcement
- `dev plugins check` warns at >1000 LOC, errors at >2000 LOC; requires `tests/`; checks README markers.
