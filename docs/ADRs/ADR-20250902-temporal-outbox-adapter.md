# ADR-20250902 — Temporal Outbox Adapter (T‑1)

Status: Accepted  
Date: 2025-09-02  
Authors: nexoscli team

## Context

We are incrementally evolving the workflow plugin toward a pluggable execution model while preserving determinism and local observability. Phase 21 (T‑2) introduced determinism guardrails and salting for idempotency. The next step (T‑1) adds the ability to offload workflow execution by emitting JSON envelopes to a local outbox directory without invoking any SDKs or network calls.

Goals:
- Add an engine switch to workflow subcommands to choose between local execution and temporal-style outbox emission.
- Ensure changes are contracts-first, observable, and non-breaking by default.
- Keep local engine behavior unchanged; outbox is additive.

## Decision

Add a new `--engine` flag to the workflow CLI:

- workflow run --engine {local|temporal} (default: local)
- workflow signal --engine {local|temporal} (default: local)

Behavior:

- Engine=local (default)
  - Unchanged: executes steps sequentially, honors determinism drift gating, records effects/history/visibility, locks, timers, children.

- Engine=temporal (new)
  - Does not execute steps locally.
  - Emits JSON envelopes to `.nexoscli/engine/outbox/`:
    - `start` (one per run)
    - One schedule per step:
      - `activity.schedule` for activity steps
      - `timer.schedule` for timer steps
      - `child.start` for child workflow steps
    - `signal` (for workflow signal)
  - Updates `.nexoscli/workflows/<id>/determinism.json` with `engine: "temporal"`.
  - Does not gate on `run.change_ids` drift (local still gates).
  - Returns a success envelope including counts (e.g., `{"scheduled": N}`).

Envelope shape (common fields):
- `op`: string — "start" | "activity.schedule" | "timer.schedule" | "child.start" | "signal"
- `workflow`: string — workflow id
- `run_id`: string — "WF-RUN-<uuid4>"
- `key`: string — workflow id (partition key)
- `name`: string (optional) — present for "signal"
- `attrs`:
  - `build_id`: string | null
  - `change_ids`: string[]
  - `spec_sha256`: string
  - `search_attributes`: object
  - `memo`: object
- `body`: object — op-specific payload (e.g., step_id, activity, args, duration_ms, child spec)
- `ts`: string — RFC3339

Persistence location: `.nexoscli/engine/outbox/` (files are pretty-printed JSON; one file per envelope with a unique name).

## Rationale

- Contracts-first: The engine flag is additive; default remains local. ToolSpecs remain compatible.
- Determinism: Temporal engine intentionally does not block on `change_ids` drift to allow outbox emission for downstream workers; local path retains gating semantics.
- Observability: All returns use `ctx.ok/ctx.err`; outbox files are observable; determinism file updated; existing visibility unchanged for minimal diffs.
- Future-proof: A worker can consume envelopes to drive a Temporal SDK or other target without changing CLI behavior.

## Alternatives Considered

- Execute Temporal SDK directly: Rejected for this phase to avoid network complexity and to keep changes minimal/scoped.
- Introduce a separate subcommand (e.g., `workflow outbox-run`): Rejected to minimize surface area; flag is simpler and consistent with existing pattern.

## Backward Compatibility

- Default engine is `local`. No change to current users unless `--engine temporal` is specified.
- Existing specs continue to validate and run locally as before.
- Tool contracts are additive; no breaking changes to required flags or names.

## Consequences

- Users can produce deterministic outbox envelopes for later consumption by a worker.
- Determinism file records the engine used. Local replay constraints remain intact.
- CI snapshot updates are required to capture new flag help text and CLI reference.

## Implementation Notes

- Code surface: `plugins/workflow/src/lib.rs`
  - Add `--engine` flag to `run` and `signal`.
  - Implement temporal branches that write envelopes via `write_outbox()`.
  - Persist `determinism.json` with `engine: "temporal"`; skip drift gating in temporal path.
- Tests:
  - `nexoscli-bin/tests/workflow_temporal_outbox.rs` covers `run` and `signal` temporal outbox behaviors.
- Docs:
  - `docs/WORKFLOWS_PLUGIN.md` updated with a "Temporal Outbox (Phase 22 — T‑1)" section.
  - Run `nexoscli dev docs build` to refresh CLI references/snapshots.

## Rollout & Verification

- `cargo test --workspace` must pass (E2E included).
- `nexoscli dev docs build` to update CLI docs/contracts snapshots (PR/update snapshots as needed).
- `nexoscli dev doctor --ci` must be green.
