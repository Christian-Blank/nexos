---
status: in-progress
plan_id: '45'
slug: rules_v0
task_id: TASK-20250905-plan-45-rules-v0-reset
started_at: 2025-09-05T21:04:38.550347Z
source: next
---
# PLAN-45 — Rules v0 (reset)

Intent
- Establish “Rules v0” as a clean reset of the rules system: a minimal, unambiguous model with deterministic tooling and E2E coverage.
- Remove legacy/back-compat behaviors; prefer generators over hand-rolled content; all outputs are JSON envelopes via ctx.ok/ctx.err.

Context
- A rules plugin exists with init/lint/export subcommands and basic tests. docs/RULES.md is currently a stub.
- Governance requires deterministic CLI contracts (ctx.ok/ctx.err), equals-form flags, and scope confinement per .clinerules.

Decision (Reset)
- Adopt a strict v0 model and CLI without shims. Any old formats/locations not recognized by v0 are ignored. The only supported content is rules/*.md with front‑matter described below.

Data Model (RuleFront)
- Fields:
  - id: RULE-<slug> (unique, non-empty)
  - title: short imperative text
  - audience: one of [human, agent, cline]
  - scope: one of [global, plugin:<name>, path:<glob>, workflow:<name>]
  - status: one of [Active, Deprecated]
  - version: semver string (e.g., 0.1.0)
  - last_reviewed: RFC3339 timestamp
  - principle: optional string (link or id)
  - owner: optional string (person/email)
- Body: free-form markdown. Suggested sections: Intent, Rule (checklist), Enforcement, Exceptions.

Directories & Artifacts
- rules/templates/rule.md: generator template
- rules/pack/core/*.md: initial seed rules
- rules/proposals/: CP-YYYYMMDD-*.md (future governance proposals)
- Exports:
  - .clinerules/compiled.md (for cline)
  - docs/RULES.md (for humans)
  - .nexoscli/rules/for-agents.md (for agents)
- All command results use ctx.ok/ctx.err JSON envelopes.

CLI Contract Surface (equals-form flags)
- nexoscli rules init [--force=true|false]
  - Creates templates and minimal core pack; deletes existing rules/ if --force=true.
  - Output: {"created": true}
- nexoscli rules lint
  - Validates front‑matter and uniqueness; ensures RFC3339 in last_reviewed.
  - Output: {"ok": boolean, "issues": [{file, message}], "files": number}
  - On failures: ctx.err with code 2; no partial writes.
- nexoscli rules export --for=<human|agent|cline> [--out=PATH]
  - Aggregates Active rules matching the audience; for=human also includes agent audience for human‑readable bundle.
  - Sorts by id ascending; deterministic output header “Rules for <audience>”.
  - Default out path if omitted:
    - cline → .clinerules/compiled.md
    - human → docs/RULES.md
    - agent → .nexoscli/rules/for-agents.md
  - Output: {"audience": "<audience>","out": "<path>"}

Determinism & Observability
- All CLI outputs are pure JSON envelopes via ctx.ok/ctx.err.
- Export ordering and content stable for the same input set.
- Prefer equals-form flags in docs/tests (e.g., --for=cline, --out=PATH).

Doctor Integration
- dev doctor --ci exposes a “rules” check:
  - If rules missing: informational (ok=true).
  - If rules initialized: can be required via --require=rules (ok=true if lint passes).
- No back‑compat paths; only rules/*.md is considered in v0.

Acceptance Criteria (binary)
- A1: rules init creates templates & core pack; force removes pre‑existing rules directory.
- A2: rules lint returns ok=true with seed pack; returns ctx.err when issues exist.
- A3: rules export produces:
  - .clinerules/compiled.md for cline and docs/RULES.md for humans (explicit or default paths).
  - Deterministic sort and header.
- A4: dev doctor --ci --checks=rules is ok both when rules missing (informational) and when required with rules initialized.
- A5: docs/RULES.md reflects exported bundle; plan moved to done; bundle.tgz created by finalize flow.
- A6: Scope confined to “rules”; no protected patterns changed.

Out of Scope
- Policy evolution and CP process beyond seed template and directory.
- Multi-language export templates or rich rendering.

Plan of Work
- Preflight:
  - Read .clinerules/compiled.md and docs/PLAYBOOK.md
  - Run:
    - cargo run -p nexoscli dev init --yes
    - cargo run -p nexoscli dev doctor --ci --checks="graph,plugins,config,events,tools" --require="plugins,graph"
  - Add “rules” scope in .clinerules/scope.json; keep changes limited per policy.
- Start Plan (select Plan 45 explicitly):
  - cargo run -p nexoscli dev plans start --output json --plan docs/project/next/PLAN-45_rules_v0.md
- Implementation:
  1) Initialize rules and verify:
     - cargo run -p nexoscli rules init --force
     - cargo run -p nexoscli rules lint
  2) Export bundles:
     - cargo run -p nexoscli rules export --for=cline --out=.clinerules/compiled.md
     - cargo run -p nexoscli rules export --for=human --out=docs/RULES.md
  3) Doctor checks:
     - cargo run -p nexoscli dev doctor --ci --checks=rules
     - cargo run -p nexoscli dev doctor --ci --checks=rules --require=rules
  4) Validation loop:
     - cargo build --workspace
     - cargo test --workspace
     - cargo run -p nexoscli dev docs build
     - cargo run -p nexoscli dev telemetry report --since 24h
  5) Commit (implementation commit, not divider):
     - git add --all
     - git commit -m "feat(rules): implement PLAN-45 rules v0 (<TASK_ID>)
- Evidence: E2E tests green (rules_init_lint_export, doctor_ci_rules)
- Deterministic exports; JSON envelopes
"
- Finalize:
  - cargo run -p nexoscli dev plans finalize --output json
  - Verify: implemented_in set; plan moved to docs/project/done/; bundle.tgz created.

Risks & Back‑compat
- This is a reset: any pre‑existing non‑conformant rule files are ignored; no migration tooling provided in v0.
- If future public contract changes occur, attach ADR and update snapshots.

References
- docs/PLAYBOOK.md, docs/TRACEPACK.md, .clinerules/Policy & scope.json
- tests: nexoscli-bin/tests/rules_init_lint_export.rs, doctor_ci_rules.rs
