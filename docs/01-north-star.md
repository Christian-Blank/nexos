---
kind: north_star
version: 0.1
id: NS-NEXOS
principles:
  - Separation of concerns, clear boundaries, single source of truth.
  - Contracts > conventions; types/traits enforce the right way.
  - Determinism + replay + observability are first-class.
  - Tests as usage examples; E2E first, unit where it buys certainty.
  - Simplicity & maintainability > cleverness & premature optimization.
  - Humans + AI are first-class users; stable JSON contracts everywhere.
  - Idempotent/deterministic generators; make the right way easy.
  - Domain-driven design; modular, loosely coupled, tightly standardized.
  - Event-driven, stateful workflows; transparent state & history.
  - Continuous improvement; small deltas guided by evidence/telemetry.
budgets:
  cli_startup_p95_ms: 75
  command_dispatch_p95_ms: 50
  event_write_p95_ms: 10
  error_rate_pct_max: 1.0
  trace_export_success_pct_min: 99.0
  mem_baseline_mb_max: 10
---

# North Star — NEXOS

**Intent:** A scalable, opinionated system that captures best practices for building with humans and AI as first class citizens, software—safe, low entropy, observable, and fast to evolve.

**Non-goals:** Tool lock-in, heavyweight governance, performance heroics before correctness.