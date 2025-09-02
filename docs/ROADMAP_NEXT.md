# NexOS CLI — Pinned Roadmap (next bites)

> We pause here. The items below are queued and will be pulled when evidence appears.

## Ready-to-pull

1) **Read-only GitHub adapter**
   - `issue import --from github --repo org/name [--state open|all]`
   - **Non-goals**: mutation, rate-limit gymnastics. Straight mapping to local issues.
   - **Acceptance**: 1:1 field map; deterministic transforms logged to events; dry-run supported.

2) **Span lifecycle guard (typed)**
   - Current RAII spans are untyped. Introduce `Phase("parse" | "validate" | "persist" | "render")` enum + helper:
     - `ctx.phase(Phase::Parse).entered()`
   - Emit `duration_ms` as span attr (computed by Drop).

3) **Typed per-plugin config**
   - Promote selected `plugins.<name>` TOML blocks to structs as needed (when >2 fields used).
   - Policy: only type a block when two commands read the same fields.

## Recently Completed
4) **Doctor: CI mode** ✅
   - `dev doctor --ci` returns non-zero on any failing readiness check.
   - Adds check: `plugins check` OK, `graph check` OK, `tools export` OK.
   - **Phase 7 Complete**: Implemented with CI aggregation and telemetry reporting.

## Recently Completed (Phase 8)
5) **Telemetry: error analytics** ✅
   - Record `error_code` in event lines → unlock error leaderboard.
   - **Phase 8 Complete**: Events now include `error_code` field; telemetry report shows error leaderboard.

6) **Telemetry: duration tracking** ✅
   - Include `dur_ms` in events from the envelope writer (computed around each command).
   - Extend telemetry report to show performance metrics.
   - **Phase 8 Complete**: Events include `dur_ms`; envelopes include `duration_ms` in meta; telemetry shows p50/p90/avg latency and slowest commands.

## New Ready-to-pull (Future enhancements)

7) **Durations per phase**
   - Emit `dur_ms_parse`, `dur_ms_persist`, etc., by capturing start/stop inside RAII parse/validate/persist/render guards.

8) **Histogram buckets**
   - Add optional `latency_bucket` to events for faster coarse aggregation.

9) **CLI table output**
   - Add `--output table` for dev telemetry report with nice columns.

10) **Error code catalog**
    - Maintain a small map code → human name.

11) **Doctor: enhanced diagnostics**
    - `dev doctor --print-plan` to show which subcommands it runs and their args.
    - `dev doctor --check bundles` → run bundle verify against current FS for the latest bundle.

## Workflow plugin — suggested next steps (post Phase 18)

12) **Timer step (durable waits)**
    - Add `timer: { duration_ms: <u64> }` step type to enable deterministic pauses within workflows (maps directly to Temporal timers).

13) **Resource locks (advisory)**
    - Introduce `resources: ["git","db:dev"]` on steps and serialize access via advisory lock files under `.nexoscli/locks/<resource>` to reduce contention locally.

14) **Child workflows**
    - Support nested specs with:
      - `child_workflow: { spec: "<path>", wait: bool, inherit_search_attributes: bool }`
      - Inherit and propagate search attributes when configured.

15) **Change IDs + Build ID enforcement**
    - Enforce runner branching rules based on recorded `change_ids` and `build_id` captured in visibility. Fail fast or require explicit upgrade paths when change guards mismatch.

## Principles (unchanged)
- Ground truth is `events.jsonl`.
- OTEL is an optional mirror; spans follow `run -> execute -> (parse, validate, persist, render)`.
- Default output comes from `.nexoscli/config.toml` or `NEXOS_OUTPUT_DEFAULT`, overridden by CLI flag.
