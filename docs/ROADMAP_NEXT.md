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

4) **Doctor: CI mode**
   - `dev doctor --ci` returns non-zero on any failing readiness check.
   - Adds check: `plugins check` OK, `graph check` OK.

## Principles (unchanged)
- Ground truth is `events.jsonl`.
- OTEL is an optional mirror; spans follow `run -> execute -> (parse, validate, persist, render)`.
- Default output comes from `.nexoscli/config.toml` or `NEXOS_OUTPUT_DEFAULT`, overridden by CLI flag.
