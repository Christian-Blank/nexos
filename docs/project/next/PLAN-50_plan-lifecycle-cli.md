# PLAN-50 — Plan Lifecycle CLI: verify, index, and enforce

Implement the missing CLI features and checks to manage, update, and enforce plan lifecycle rules across
`docs/project/{next,current,done,archived,backlog,paused,dumped}` so the streamlined development flow is fully automated and CI‑enforceable.

⸻

## 1) Problem

- Plans move across lifecycle folders; rules exist in READMEs and ADRs, but automation is partial.
- Indexes in subfolder READMEs (AUTO blocks) should be deterministically refreshed.
- CI should gate policy violations (e.g., multiple plans in current/, numbered files in backlog/, naming drift in next/),
  but today these checks are not available as a dedicated “plans” check.

## 2) Goals

- Add CLI surfaces to verify plan folder invariants and refresh indexes deterministically.
- Provide a dedicated doctor check for plans so CI can gate lifecycle violations.
- Keep diffs minimal, deterministic; do not alter existing start/finalize semantics.

## 3) Scope (v0)

- Only lifecycle management of docs/project/* folders and their README indexes.
- Read/write under docs/project/*; update only AUTO blocks (no manual section rewrites).
- No cross-repo/network operations; no GitHub API; no content outside project folders.

Out-of-scope (v0):
- Automated moving/renaming of plan files between folders.
- Editing plan front-matter fields (that remains under plans start/finalize).
- Strict immutability enforcement beyond warning-level (see risks).

## 4) Proposed CLI surfaces (new)

A) Verify lifecycle rules
```
nexoscli dev plans verify [--path docs/project] [--output json]
```
Validates and returns a ctx.ok envelope with:
- items: [
    { name: "current", ok, errors[], warnings[] },
    { name: "next", ok, errors[], warnings[] },
    { name: "done", ok, errors[], warnings[] },
    { name: "backlog", ok, errors[], warnings[] },
    { name: "archived", ok, errors[], warnings[] },
    { name: "paused", ok, errors[], warnings[] },
    { name: "dumped", ok, errors[], warnings[] }
  ]

Rules (v0):
- current/: exactly one `PLAN-*.md` present; error otherwise.
- next/: all files named `PLAN-<id>_<slug>.md` (preferred) or legacy `plan-...` (warning); no duplicate ids (error).
- done/: warn if CLI-managed finalize fields appear mutated post-finalize (best-effort detection).
- backlog/: no numbered plans; warn if `PLAN-*.md` is detected.
- archived/: informational only.
- paused/: zero or more; direct `paused/ → next/` is discouraged (informational).
- dumped/: informational only.

B) Deterministic indexes builder
```
nexoscli dev plans index build [--path docs/project] [--dry-run] [--output json]
```
Refreshes AUTO (id=plans-index) in each subfolder README:
- next/: list `PLAN-*.md` (lexicographic)
- current/: list files (should be exactly one)
- done/: list all plan files (lexicographic)
- backlog/: list plan-like entries that are not numbered (e.g., conceptual documents)
- archived/, paused/, dumped/: list present plan files if any
- Must be idempotent; with `--dry-run` prints would-be changes.

C) Doctor integration
```
nexoscli dev doctor --ci --checks plans [--output json]
```
Adds the “plans” composite check which invokes `dev plans verify` and reports a summarized, CI-friendly result.
Integrate into existing `doctor` so we can run `--checks="repo,plans"`.

Optional helpers (phase 2/follow-up):
- `nexoscli dev plans fix --indexes` → alias for `index build` (no additional behavior).

## 5) Behavior details

- The verify command must never modify files; only report.
- The index build command must never edit outside AUTO (id=plans-index) blocks.
- All file list outputs must use relative links and stable lexicographic ordering.
- All commands return ctx.ok/ctx.err JSON envelopes for observability.

## 6) Acceptance Criteria

- `nexoscli dev plans verify` returns ok with items per folder on a clean repository.
- `nexoscli dev plans index build` updates no files on second run (idempotence proven).
- `nexoscli dev doctor --ci --checks plans` returns OK on the current repository state.
- For contrived violations in tests:
  - Duplicate plan ids in `next/` → error in verify output.
  - Multiple files in `current/` → error in verify output.
  - Numbered plans found in `backlog/` → warning in verify output.

## 7) Tests (E2E via nexoscli-bin/tests)

- dev_plans_verify_ok.rs — happy path on fixture with valid layout.
- dev_plans_verify_violations.rs — duplicate ids, multiple current, numbered in backlog.
- dev_plans_index_build_idempotent.rs — run twice and assert no changes on second pass.
- doctor_ci_plans_ok.rs — include “plans” check in doctor and assert OK envelope.

## 8) Risks & mitigations

- False positive on done/ immutability checks:
  - v0 as warning-only; refine heuristics later or require implemented_in anchoring.
- Index drift via manual edits:
  - Make builder idempotent; discourage manual edits in AUTO blocks and rely on tool.
- Scope creep:
  - Keep v0 to verify + index + doctor integration; no auto-migration of files.

## 9) Work Plan

1) CLI plumbing:
   - Add commands:
     - dev::plans::verify
     - dev::plans::index::build
   - Add doctor aggregator “plans”.

2) Verifier:
   - Scan directory tree and build typed findings.
   - Evaluate rules per lifecycle folder.

3) Index builder:
   - Enumerate plans per folder; render Markdown link lists into AUTO (id=plans-index).
   - Support `--dry-run`; compute digests for change detection.

4) Tests:
   - E2E tests that assert JSON envelopes and stable outputs.

5) Docs:
   - Update docs/WORKFLOWS_TASK.md and docs/WORKFLOWS_PLUGIN.md with new commands.
   - Ensure docs/project/* READMEs reference these commands where relevant.

## 10) Dependencies

- README System v0 (ADR-20250907-readme-system-v0)
- Co-located rules & workflows ADR (ADR-20250907-co-located-folder-rules-and-workflows)
- Existing `dev repo check` and `doctor` framework

## 11) Done signal

- All acceptance criteria satisfied (commands + tests).
- CI green with “plans” included in doctor checks.
- Minimal diffs; AUTO updates only within markers.
