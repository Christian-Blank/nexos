# ADR-0001: CLI Contracts & Composition

- Status: Accepted
- Date: 2025-09-01

## Decision

1) **Compose Clap before parsing**; plugins augment the tree under their namespace → one parser, correct help/completions/man pages.
2) **Static plugin linking** via Cargo features in v0; no dynamic loading.
3) **JSON is the CLI boundary**. `--output json` emits a single object `{ data, errors, meta }`. Text/yaml are renderings only.
4) **Determinism providers** injected via `CommandContext` (Clock, Entropy, Io). No direct `SystemTime::now()` or `thread_rng()` in prod paths.
5) **Event log**: append-only JSONL at `.nexoscli/log/events.jsonl` with a content hash per event.
6) **Observability**: `tracing` spans for parse/validate/execute/persist/render; OTLP exporter off by default, enabled via env.
7) **Tools manifest**: `nexoscli tools export` will emit JSON Schemas for all commands (AI-first contract).
8) **Python UV**: For python scripts use UV env where required.

## Consequences

+ Reliable help/completions/docs; reproducible runs; stable machine I/O.
- Rebuilds required to add plugins (acceptable for strictness in v0).
