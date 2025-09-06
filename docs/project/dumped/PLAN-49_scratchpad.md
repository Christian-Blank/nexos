# PLAN-49 — README System v0 — Scratchpad

Task: TASK-20250906-graph
Plan: PLAN-49 (readme-restructure)
Branch: task/49-readme-restructure
Scope: cline-workflows (docs-focused; narrow exceptions noted below)
Started: 2025-09-06T22:04:06Z

This scratchpad logs observations, frictions, inconsistencies, inefficiencies, ideas, decisions, and follow-ups while implementing PLAN-49. It includes an execution checklist mapped 1:1 to the Start New Task workflow and acceptance criteria from the plan.

---

## Observations
- Repo already contains most domain READMEs; they lack readme.v0 front-matter and AUTO blocks.
- Required folder `testdata/` is missing.
- `.nexoscli/repo_policy.yaml` exists (separate); adding `.nexoscli/readme.policy.yaml` will be additive for README system v0.
- `docs/exec_graph.yaml` is present and allowed (YAML allowlist).

## Frictions
- None yet. Will log if the `dev repo check --fix-readmes` cannot auto-fix missing front-matter or if term wrapper adds significant overhead.

## Inconsistencies
- Some domain READMEs contain rich prose not aligned to Purpose/In/Out/Interfaces sections; will minimally add sections without removing content.

## Inefficiencies
- Repeated manual front-matter scaffolding across many files. Follow-up CLI (`readme build/verify`) would reduce toil.

## Ideas
- Implement `nexoscli dev readme build` to materialize AUTO children blocks deterministically from policy/config.
- `nexoscli dev readme verify` for front-matter and graph link validation.

## Decisions
- Use cline-workflows scope and document narrow exceptions:
  - Add `.nexoscli/readme.policy.yaml` at repo root (config).
  - Add `plugins/.readme.yaml` (override example outside docs).
  - Create `testdata/README.md`.
- Preserve existing prose and prepend front-matter + minimal required sections + AUTO placeholders.

## Follow-ups
- Consider a CP/ADR if scope policy requires explicit approval for `.nexoscli/` or new folder creation.

---

## Execution Checklist (Start New Task — Next → Current v1)

0) Pre-flight
- [x] Read workflow docs (start_new_task.workflow.md)
- [x] Read guardrails (.clinerules/compiled.md, protected-patterns.md, scope.json)
- [x] Sync main and configure push.autoSetupRemote
- [x] Create branch (task/49-readme-restructure) and set upstream
- [x] Start plan using CLI-native flow (see step 1)
- [ ] Baseline health: `dev init --yes` and `dev doctor --ci --checks="graph,plugins,config,events,tools" --require="plugins,graph"`
- [x] Choose scope label: cline-workflows (docs-focused)

Trigger: Complete when branch pushed and baseline checks executed successfully.

1) Start plan (creates+starts task, moves next → current)
- [x] Run: `nexoscli dev plans start --output json`
- [x] Record task_id in this scratchpad

Trigger: Plan moved to docs/project/current and task_id assigned.

2) Implementation loop (deterministic, minimal diffs)
- [ ] Add `.nexoscli/readme.policy.yaml` per Plan-49 Section 6
- [ ] Create `plugins/.readme.yaml` (override example)
- [ ] Create `testdata/README.md` (domain template)
- [ ] Update root `README.md`: prepend readme.v0 front-matter, include scripts/README.md in children, add AUTO children block
- [ ] Update domain READMEs (docs, plugins, nexoscli-core, nexoscli-bin, rules, examples, contracts, adapters, scripts): prepend front-matter, parents ["../README.md"], children [], add AUTO children block; add Purpose/In/Out/Interfaces sections
- [ ] Update plugin leaf READMEs (plugins/*): prepend leaf front-matter, parents ["../README.md","../../README.md"], children: []
- [ ] Validate frequently:
  - cargo build --workspace
  - cargo test --workspace
  - nexoscli dev docs build
  - nexoscli dev repo check --fix-readmes --output json
  - nexoscli dev doctor --ci --checks repo --output json

Trigger: Repo check and doctor CI return OK; diffs minimal.

3) Implementation commit (not divider)
- [ ] `git add --all`
- [ ] Conventional commit:
  feat(cline-workflows): implement PLAN-49 readme-restructure (TASK-20250906-graph)
  - README system v0: policy + front-matter + AUTO blocks
  - Evidence: repo check + doctor CI green
- [ ] Push branch

Trigger: Commit created and pushed.

4) Human Review
- [ ] Summarize changes and notable scratchpad entries

Trigger: Human signs off.

5) Finalize Task
- [ ] Run Finalize Task workflow to create divider and move plan to done/

Trigger: Plan moved to done/, front-matter updated accordingly.

---

## Acceptance Criteria Mapping
- [ ] `.nexoscli/readme.policy.yaml` present with specified fields; no `.readme.yaml` under docs/.
- [ ] All required folders have README.md with readme.v0 front-matter; root/domain/leaf templates applied.
- [ ] Parent/children relationships set; AUTO markers present; no manual edits within AUTO blocks.
- [ ] `nexoscli dev repo check` passes; `nexoscli dev doctor --ci --checks repo` OK.
- [ ] Protected patterns untouched; no code added under docs/.
- [ ] Minimal diffs; existing prose preserved.
