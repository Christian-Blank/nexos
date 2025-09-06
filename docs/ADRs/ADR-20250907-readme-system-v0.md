# ADR-20250907 — README System v0: Deterministic, Graph‑Linked, DRI‑Operated

Status: accepted  
Deciders: DRI (docs), DRI (core), DRI (dev)  
Date: 2025-09-07

## Context

We want all documentation entry points (README.md files) to be:

- Deterministic and graph‑linked (parents/children explicitly encoded).
- Machine‑verifiable (front‑matter schema; staleness SLOs; markers for generated blocks).
- Enforceable in CI (repo policy and doctor checks).
- Human‑effective (clear Purpose/Scope/Interfaces; minimal; navigable).
- Ready for automation (future CLI `readme verify/build/stale`).

The repo refactor established canonical locations (examples/workflows, adapters/temporal-worker, contracts/schemas). Plan‑48 introduced `dev repo check` and “no code in docs” guardrails. Plan‑49 designed the README system v0 and its policy.

## Decision

Adopt README System v0 with these non‑negotiable rules:

1) Presence: All major folders have a README.md.  
2) Front‑matter: Every README starts with YAML conforming to `readme.v0`.  
3) Graph links: Parents and children declared in front‑matter (explicit or policy‑inherited).  
4) Boundaries: Each README includes Purpose, In‑scope, Out‑of‑scope, Interfaces.  
5) Canonicality: If conflicts arise, the README’s `source_of_truth` wins.  
6) Auto blocks: Generated content only inside `<!-- BEGIN: AUTO (id=…) -->…<!-- END: AUTO -->`. No manual edits within.  
7) SLOs: last_reviewed ≤ 45 commits (domain) and ≤ 90 commits (leaf).  
8) Docs contain no executable code (guardrail enforced).  
9) One concern: README describes its folder. Cross‑domain content goes to the nearest shared parent.

Front‑matter schema (readme.v0) includes: schema, title, summary, kind (root|domain|leaf), owner, status, version, last_reviewed, parents, children, source_of_truth, in_scope, out_of_scope, inputs, outputs, tags, and optional `auto` (children_glob, dir_tree_depth, badges).

Repo policy file: `.nexoscli/readme.policy.yaml` defines defaults and checks:

- version: v0.1  
- default_kind: domain  
- default_auto: children_glob ["*/README.md"], dir_tree_depth 1, badges true  
- required_folders: ["docs","plugins","nexoscli-core","nexoscli-bin","rules","examples","contracts","adapters","scripts","testdata"]  
- slo: 45/90 days  
- lint: require_frontmatter, require_scope_sections, require_graph_links

## Consequences

- Deterministic graph reconstruction from READMEs alone.  
- Low drift: SLOs and CI checks keep READMEs fresh.  
- Tools can safely refresh inside AUTO blocks without touching manual text.  
- Slight authoring overhead (front‑matter + sections), offset by clarity and automation benefits.  
- Enabling future CLI: `dev readme verify/build/stale`.

## Rejected Alternatives

- Central registry for the doc graph (tends to drift; creates single choke point).  
- Full-regeneration (brittle diffs; erodes human‑authored sections).  
- Ad‑hoc markdown without schema (not machine‑verifiable or enforceable).

## Implementation

- Policy: `.nexoscli/readme.policy.yaml` added.  
- Root and domain/leaf READMEs updated with `readme.v0` schema, parents/children, and AUTO blocks.  
- “No .readme.yaml under docs/” respected; optional override only outside docs (e.g., `plugins/.readme.yaml`).  
- Plan folders under `docs/project/*` will include structured Rules/Workflows/Checks and an index AUTO block (`id=plans-index`) to list contained plan files deterministically.

## Status and Rollout

- Accepted; implemented under branch `task/49-readme-restructure` with `dev repo check` and `doctor --ci --checks repo` green.  
- Existing READMEs preserved; scope sections added minimally.  
- Future CLI can verify/build indexes and validate graph edges.

## Future Work

- `nexoscli dev readme verify` and `nexoscli dev readme build` to refresh AUTO blocks (children lists; plan indexes).  
- CI to assert single file in `docs/project/current/`; naming in `next/`; immutability in `done/`.  
- Staleness report: list READMEs exceeding SLO thresholds.

## References

- Plan: `docs/project/current/PLAN-49_readme-restructure.md`  
- Policy: `.nexoscli/readme.policy.yaml`  
- Guardrails: `.clinerules/protected-patterns.md`, `.clinerules/compiled.md`  
- Checks: `nexoscli dev repo check`, `nexoscli dev doctor --ci --checks repo`  
- Prior ADRs: `docs/ADRs/0001-cli-contracts.md`, `ADR-20250903-tracepack-bags-v0.2.md`, others.
