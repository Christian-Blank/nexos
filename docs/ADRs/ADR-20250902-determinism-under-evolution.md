# ADR-20250902: Workflow Determinism Under Evolution (T-2)

Status: Accepted  
Date: 2025-09-02  
Authors: Workflow Plugin DRI  
Scope: workflow-plugin (docs/**, plugins/workflow/**, nexoscli-bin/tests/**)

## Context

We are preparing for a Temporal-ready engine with stable contracts and deterministic replay. Prior phases delivered timers, signals/queries, visibility, resource locks, and child workflows. T-2 focuses on determinism under evolution so that changes to workflow-visible behavior are consciously gated and observable.

Specifically, we introduce:
- Change-IDs: explicit version fences that must be bumped when making workflow-visible changes.
- Build-ID: an identifier for routing/compatibility, stored alongside visibility.
- Validate: a CLI command to validate spec invariants before execution.
- Gating: local engine `workflow run` fails on `change_ids` drift unless explicitly overridden.
- Input hashing salt: prevent false idempotent skips when determinism knobs change.

## Decision

1) CLI Surface Changes
- New subcommand:
  - `workflow validate --spec <YAML>`
- New flags on `workflow run`:
  - `--allow-change-ids-drift` (bool)
  - `--build-id <string>`

2) Determinism Metadata
- Persist determinism to `.nexoscli/workflows/<id>/determinism.json`:
  - `{ build_id, change_ids, spec_sha256, first_run_at, last_run_at, engine }`
- Enrich visibility with `build_id` and `used_change_ids` (already present in schema).
- Gate `workflow run` on `change_ids` set drift unless `--allow-change-ids-drift` is provided; append a `change_ids_drift` event to history when detected.

3) Input Hashing Salt
- Salt `EffectRecord.input_sha` with `run.build_id` and `run.change_ids` so idempotent steps do not falsely skip after determinism changes.

4) Validation
- `workflow validate` performs schema + invariants checks:
  - unique step ids
  - exactly one of `activity|timer|child_workflow`
  - resource names charset: `[a-z0-9:\-]+`
  - `change_ids` format: `^[A-Z][A-Z0-9_\-]{1,63}$`
  - warns on empty steps
- The existing `read_spec()` still enforces correctness for execution; `validate` returns a report envelope for preflight feedback.

## Alternatives Considered

- Implicit drift acceptance: rejected due to lack of observability and risk of silent behavior changes.
- Storing determinism state elsewhere (e.g., effects dir): rejected to keep metadata co-located with workflow instance lifecycle under `.nexoscli/workflows/<id>`.

## Consequences

- Contracts Impact:
  - CLI adds a new subcommand and two flags. `docs/CLI_REFERENCE.md` and contracts snapshots may change accordingly via `nexoscli dev docs build`.
  - No breaking change for existing specs; determinism knobs are optional.
- Replay Safety:
  - Runs fail fast when `change_ids` drift is detected; override is explicit and recorded.
  - Input hash salting ensures idempotent skips don’t accidentally reuse old effects across determinism changes.
- Observability:
  - Determinism.json carries spec hash and time bounds for first/last run.
  - History entries document `change_ids_drift` events.

## Migration

- Existing specs continue to run. To adopt determinism, set `run.build_id` and `run.change_ids` in the spec.
- On first successful run after adding change_ids, determinism.json is created.
- If later changes require bumping `change_ids`, either:
  - Update the spec and re-run with `--allow-change-ids-drift` one time to accept and update determinism.json; or
  - Fail fast (default) to catch unexpected drifts.

## Acceptance Criteria (met)

- `workflow validate` returns an envelope report with errors/warnings.
- First successful run writes `determinism.json` with expected fields.
- Subsequent runs fail on `change_ids` set drift unless `--allow-change-ids-drift` is provided; history logs a `change_ids_drift` event.
- Visibility includes `build_id` and `used_change_ids`.
- Input hashing salted with determinism knobs to prevent false idempotent skips.

## Tests

- Added E2E: `nexoscli-bin/tests/workflow_change_ids.rs`:
  - Run with `["CH_V1"]` -> success
  - Change to `["CH_V2"]` -> failure (err envelope, exit code 66)
  - Re-run with `--allow-change-ids-drift` -> success

## Follow-ups

- If `docs/contracts/**` snapshot drift is detected in CI after `nexoscli dev docs build`, proceed with the dedicated “update snapshots” PR lane.
- When adding Temporal engine mode, reuse determinism fields for routing and version gating.
