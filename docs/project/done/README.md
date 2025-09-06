---
schema: readme.v0
title: "project/done"
summary: "Completed plans (immutable history) finalized via the CLI."
kind: domain
owner: "@docs/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-07T00:00:00Z"
parents: ["../README.md"]
children: []
source_of_truth:
  - "./"
in_scope:
  - "Archive of completed plans"
  - "Immutable history of decisions and implementations"
out_of_scope:
  - "Manual edits to completed plan metadata"
inputs:
  - "PLAN-<id>_<slug>.md files moved here by CLI finalize"
outputs:
  - "Stable plan history for reference and audits"
tags: ["plans","done"]
auto:
  children_glob: []
  dir_tree_depth: 0
---

# Done

Completed plans. Files here are an immutable history; do not edit plan content beyond what the CLI writes on finalize.

## Purpose

Preserve finalized plans with their recorded metadata (implemented_in, completed_at) as the audit trail of work delivered.

## In‑scope

- Referencing completed plans
- Linking plans in ADRs and docs

## Out‑of‑scope

- Manual modifications of CLI-maintained front‑matter
- Moving plans out of this folder

## Workflows

- List completed plans:
  ```
  nexoscli dev plans list --where done --output json
  ```

## Rules

- Plans are finalized via CLI:
  ```
  nexoscli dev plans finalize --output json
  ```
- Treat contents as immutable history. Use ADRs/CPs for any superseding changes.

## Checks & Gates

- Repo policy and readmes:
  ```
  nexoscli dev repo check --fix-readmes --output json
  ```
- CI repo gate:
  ```
  nexoscli dev doctor --ci --checks repo --output json
  ```

## Interfaces

- Inputs: plan files finalized from `current/`
- Outputs: historical ledger of completed plans

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
- [plan-26_contract_freeze.md](./plan-26_contract_freeze.md)
- [plan-27_doctor_checks.md](./plan-27_doctor_checks.md)
- [plan-28_doctor-events-schema.md](./plan-28_doctor-events-schema.md)
- [plan-29_event-log-rotation.md](./plan-29_event-log-rotation.md)
- [plan-30_bag-verify.md](./plan-30_bag-verify.md)
- [plan-31_file-change-topic.md](./plan-31_file-change-topic.md)
- [plan-32_bag-explain.md](./plan-32_bag-explain.md)
- [plan-33_events-schema-and-normalizer.md](./plan-33_events-schema-and-normalizer.md)
- [plan-33-2_schemas-bundles-and-validators.md](./plan-33-2_schemas-bundles-and-validators.md)
- [PLAN-34_plan-and-dev-cli.md](./PLAN-34_plan-and-dev-cli.md)
- [plan-34_schemas-bundles-and-validators.md](./plan-34_schemas-bundles-and-validators.md)
- [PLAN-35_fileops-normalizer.md](./PLAN-35_fileops-normalizer.md)
- [PLAN-36_export-helpers-csv.md](./PLAN-36_export-helpers-csv.md)
- [PLAN-37_annotate-and-export.md](./PLAN-37_annotate-and-export.md)
- [PLAN-38_cline-correlation-handling.md](./PLAN-38_cline-correlation-handling.md)
- [PLAN-39_agent-grade-cline-capture.md](./PLAN-39_agent-grade-cline-capture.md)
- [PLAN-40-41_env-and-repo.md](./PLAN-40-41_env-and-repo.md)
- [PLAN-40-44_env-and-repo-snapshot.md](./PLAN-40-44_env-and-repo-snapshot.md)
- [PLAN-42-43_analyst-ux.md](./PLAN-42-43_analyst-ux.md)
- [PLAN-44_finalize-v0.md](./PLAN-44_finalize-v0.md)
- [PLAN-45_rules_v0.md](./PLAN-45_rules_v0.md)
- [PLAN-47_refactor-and-stabilize.md](./PLAN-47_refactor-and-stabilize.md)
- [PLAN-48_repo-check.md](./PLAN-48_repo-check.md)
<!-- END: AUTO -->
