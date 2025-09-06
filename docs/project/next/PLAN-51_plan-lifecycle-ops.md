# PLAN-51 — Plan Lifecycle Ops/API: transactional moves and front‑matter updates

Define and implement deterministic, auditable CLI operations for moving plans across lifecycle stages (start, pause, resume, finalize, archive, dump, requeue), including front‑matter updates (e.g., `implemented_in`), safety preconditions, dry‑run, and transactional behaviors. Elevate lifecycle to an explicit API with envelopes, not ad‑hoc file moves.

⸻

## 1) Problem

- Today, folder placement implies state; richer semantics (preconditions, SHA anchoring, reconciliation, header updates) are not formalized as dedicated ops.
- Finalize must accept a commit SHA (or HEAD) and atomically update front‑matter (implemented_in, completed_at, status=done) and then move the plan to `done/`.
- Pausing/resuming/archiving/dumping should be first‑class operations with safety checks, not just manual relocations.

## 2) Goals

- Provide explicit CLI operations for lifecycle transitions, with:
  - Preconditions and safety checks (single current, uniqueness in next, etc.)
  - Dry‑run and idempotence
  - Front‑matter mutations only within defined ops
  - JSON envelopes (ctx.ok/ctx.err) and robust event emission
- Keep folder placement a projection of state, enforced by the API.

## 3) Scope (v0)

In scope:
- Lifecycle ops: `finalize` (enhanced), `pause`, `resume`, `archive`, `dump`, `requeue` (paused → backlog)
- Optional rename op (v0.1) for plan_id/slug with collision checks
- Read/write limited to `docs/project/*` and plan front‑matter; AUTO index refresh optional via flag

Out of scope (v0):
- Automated mass migrations or cross‑repo operations
- Editing plan content beyond allowed header fields
- Enforcing strict immutability history beyond warning (v0)

## 4) Proposed CLI surfaces (new/enhanced)

A) Finalize (enhanced)
```
nexoscli dev plans finalize [--plan PATH] [--implemented-in SHA|HEAD] [--bundle-out PATH] [--dry-run] --output json
```
Behavior:
- Require exactly one plan in `current/` unless `--plan` is provided and valid.
- Resolve SHA: if provided, validate; else resolve HEAD.
- Update front‑matter:
  - `status: done`
  - `completed_at: <now RFC3339>`
  - `implemented_in: <SHA>`
- End active task and optionally build bundle (`--bundle-out`).
- Move file to `done/`.
- Idempotent; dry‑run prints intended changes only.
- Envelope contains before/after summary + paths.

B) Pause
```
nexoscli dev plans pause [--plan PATH] [--dry-run] --output json
```
- Move `current/` → `paused/`.
- Enforce single current; optional rationale header note.
- No other front‑matter changes.

C) Resume
```
nexoscli dev plans resume [--plan PATH] [--dry-run] --output json
```
- Move `paused/` → `current/`.
- Enforce single current (error if `current/` non‑empty).
- Reconciliation step before move:
  - Invoke repo/plan checks to detect drift (doctor repo + plans checks).
  - Envelope includes reconciliation summary.

D) Archive
```
nexoscli dev plans archive --plan PATH [--reason TEXT] [--dry-run] --output json
```
- Move to `archived/` with optional rationale header; no other mutations.

E) Dump
```
nexoscli dev plans dump --plan PATH [--reason TEXT] [--dry-run] --output json
```
- Move to `dumped/` with optional rationale; no other mutations.

F) Requeue (paused → backlog)
```
nexoscli dev plans requeue --plan PATH [--dry-run] --output json
```
- Move `paused/` → `backlog/` for reprioritization.
- Warn on numbered filenames; recommend rename (defer rename automation to v0.1).

G) Rename (optional v0.1)
```
nexoscli dev plans rename --plan PATH --plan-id <id> --slug <slug> [--dry-run] --output json
```
- Rename file and front‑matter consistently.
- Re‑validate uniqueness (e.g., in next/).
- Maintain lexicographic ordering for deterministic indexes.

## 5) Safety & transactions

- Preconditions:
  - `current/` must have exactly one plan for `finalize/pause` unless `--plan` given.
  - No duplicate ids in `next/` if rename/requeue can affect id space.
- Dry‑run for every op; print file graph changes and front‑matter diffs.
- Atomicity:
  - Create tmp + rename or write‑then‑rename; rollback on error with clear envelope.
- Observability:
  - JSON envelopes; event emission with content digests, affected paths, and front‑matter diffs.

## 6) Front‑matter mutations (ops‑only)

- finalize: `status`, `completed_at`, `implemented_in`
- pause/resume/archive/dump/requeue: file movement only; optional “rationale” header tag/comment
- rename: `plan_id` + `slug` + filename alignment; re‑validate indexes

## 7) Integration with verify/index/doctor

- Keep PLAN‑50 commands:
  - `dev plans verify` for invariant evaluation
  - `dev plans index build` for AUTO (id=plans-index) refresh
- Optionally add `--refresh-indexes` to lifecycle ops to run index build post‑op.
- Doctor:
  - `nexoscli dev doctor --ci --checks plans` uses verify to gate lifecycle rules.

## 8) Acceptance Criteria

- Finalize:
  - Sets `implemented_in` to HEAD or provided SHA, updates `completed_at`, moves to `done/`, ends task; dry‑run idempotent.
- Pause/Resume/Archive/Dump/Requeue:
  - Respect preconditions; envelopes explain actions; dry‑run idempotent, no writes.
- All ops return deterministic JSON envelopes with before/after and affected files.

## 9) Tests (E2E via nexoscli-bin/tests)

- finalize_ok_head.rs, finalize_ok_sha.rs, finalize_dry_run.rs
- pause_resume_ok.rs; resume_conflict_current_nonempty.rs
- archive_ok.rs, dump_ok.rs, requeue_ok.rs
- (optional) rename_ok.rs, rename_collision_error.rs
- Full lifecycle E2E: start → pause → resume → finalize → done

## 10) Risks & mitigations

- Manual edits outside ops:
  - Communicate ops‑only mutation policy; verify warns on unexpected drift.
- SHA mismatch / race:
  - Validate SHA at op start; optionally assert clean working tree or warn.
- Scope creep:
  - Limit v0 to core ops; leave rename strictness and done immutability “hard” checks to follow‑up.

## 11) Work Plan

1) Command plumbing:
   - Implement dev::plans::{finalize,pause,resume,archive,dump,requeue} with shared utils
2) Preconditions & diffing:
   - Directory scan, current/ singleton assert, ID collision checks
   - Header parsing/writing with deterministic serializer
3) Atomic fs ops:
   - Temp + swap with rollback
4) Reconciliation (resume):
   - Call “verify” + “repo check”; attach summary into envelope
5) Tests:
   - E2E and golden outputs for JSON envelopes, idempotence
6) Docs:
   - Update READMEs/workflows with canonical CLI examples

## 12) Dependencies

- ADR‑20250907‑readme‑system‑v0
- ADR‑20250907‑co‑located‑folder‑rules‑and‑workflows
- PLAN‑50 (verify/index/doctor integration)

## 13) Done signal

- All ops available with dry‑run and envelopes
- Tests pass; doctor `--checks plans` green
- Idempotence of index build and ops validated on re‑run
