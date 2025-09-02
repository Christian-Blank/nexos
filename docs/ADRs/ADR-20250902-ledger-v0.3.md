# ADR-20250902 — Event Ledger v0.3 (Hash-Chained Events with Per-Run Manifests)

Status: Accepted
Date: 2025-09-02
Authors: nexoscli team

## Context

NEXOS currently emits structured events to `.nexoscli/log/events.jsonl` and exposes JSON envelopes via `ctx.ok/ctx.err`. Events include a `content_hash` but are not tamper-evident as a chain and do not maintain per-run integrity manifests. Replay and observability are core goals; we need a deterministic, verifiable ledger to support trustworthy replay, metrics, and CI gating.

Constraints and guardrails:
- Contracts-first. Do not break CLI or JSON envelope contracts without ADRs and snapshot updates.
- Protected core files (e.g., `nexoscli-core/src/{envelope.rs,event.rs,plugin.rs,engine.rs}`) require an ADR before any edits.
- Minimal diffs; centralize changes in the event writer code path to avoid duplicated logic.

## Decision

Introduce Event Ledger v0.3 and wire it into the core event writer:

1) Hash-Chained Events
- Each event line is extended with:
  - `schema: "event.v0.3"`
  - `seq` (1..N per run), `prev` (chain hash of previous line or empty for first),
  - `content_hash` computed over a canonical, stable subset of the event,
  - `chain_hash = sha256(prev + content_hash)`.
- Write to both:
  - Global append-only log: `.nexoscli/log/events.jsonl`
  - Per-run file: `.nexoscli/log/runs/<RUN-ID>.jsonl`

2) Per-Run Manifest
- On run completion (Drop), write `.nexoscli/log/runs/<RUN-ID>.manifest.json`:
  - `version: "0.2"`, `run_id`, `count`, `first_seq`, `last_seq`,
  - `first_hash`, `last_hash`, `time_range: {from,to}`, `chain_ok: true` (best-effort).

3) Canonicalization
- `content_hash = sha256(canonical_event)` excluding chain fields. Initial canonical subset matches current writer payload to minimize churn:
  `{ plugin, command, exit_code, dur_ms, data }`
- We may refine canonicalization over time (e.g., include `args_hash`, structured `attrs`) without changing chain semantics.

4) Rollout Gate
- Gate behind `NEXOS_LEDGER=0/1`. Enabled by default; setting `NEXOS_LEDGER=0` reverts to previous behavior (no v0.3 fields, no per-run files).
- Ensures safe rollout and quick fallback.

5) Documentation and Schemas
- Add `docs/SPECS/event_v0.3.schema.json` and `docs/SPECS/run_manifest_v0.2.schema.json`.
- Update `docs/OBSERVABILITY.md` with Event v0.3 section.
- Update `docs/TRACEPACK.md` to v0.2 (manifest and `chain_ok`).

## Alternatives Considered

- Continue with v0.2 (content-only hashing): rejected; not tamper-evident, insufficient for replay guarantees.
- Implement chaining outside core writer (e.g., plugin or post-processor): rejected; risks divergence and non-determinism.
- Change stdout envelope shapes: rejected for Phase 1; keep user-facing contracts stable.

## Consequences

- Internal event lines gain `schema/seq/prev/chain_hash` when enabled.
- Per-run files and manifests provide portable integrity anchors.
- Minimal diffs in the core writer; aggregates/consumers can adopt v0.3 progressively.
- Future phases (Replay Verify, Doctor gating, Telemetry metrics) rely on this ledger.

## Implementation

- Add `nexoscli-core/src/ledger.rs`:
  - `Ledger::open(".nexoscli", run_id)` creates/wires paths.
  - `append(...)` computes `content_hash` and `chain_hash`, writes to per-run and global logs, tracks first/last metadata.
  - `Drop` writes per-run manifest.
- Patch protected `nexoscli-core/src/event.rs` (EventWriter):
  - Initialize a `Ledger` when `NEXOS_LEDGER != "0"`.
  - After building the event content, call `ledger.append(...)` to add v0.3 fields and mirror writes.
  - Include v0.3 fields in the line written when ledger is enabled; otherwise keep prior v0.2 line.

## Rollout and Validation

- E2E: Add `nexoscli-bin/tests/envelope_hash_chain.rs` to assert:
  - `seq` increments, `prev` linkage, and `chain_hash == sha256(prev + content_hash)`.
  - Manifest exists with expected first/last.
- Validation loop:
  - `cargo build --workspace`
  - `cargo test --workspace`
  - `nexoscli dev docs build`
  - `nexoscli dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph`

## Backwards Compatibility

- With `NEXOS_LEDGER=0` (or by default if later required), writer produces previous v0.2 shape only.
- No changes to stdout JSON envelope contract (`ctx.ok/ctx.err`) in Phase 1.

## Status

Accepted for Phase 1 (scope: observability). Subsequent phases:
- Phase 2 (scope: replay): `replay verify` subcommand and TracePack manifest integration.
- Phase 3 (scope: dev-plugin): telemetry metrics slice and `doctor --require events_integrity`.
