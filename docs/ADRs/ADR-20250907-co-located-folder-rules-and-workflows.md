# ADR-20250907 — Co-Locate Folder-Specific Rules and Workflows in README.md

Status: accepted  
Deciders: DRI (docs), DRI (dev), DRI (core)  
Date: 2025-09-07

## Context

Rules and operational workflows often drift when centralized or scattered across multiple documents. This causes:
- Telephone/copy effects across teams and tools
- Lower discoverability for humans and LLMs
- Fragile enforcement and CI gates due to ambiguity
- Extra overhead for changing the “source of truth”

With README System v0 (ADR-20250907-readme-system-v0) we established deterministic, machine‑verifiable READMEs with a stable front‑matter schema (readme.v0), AUTO blocks, and repo policy defaults. To complete the model, we now standardize the co‑location of rules and workflows inside the README of the folder where they apply.

## Decision

Adopt the “rules live where they apply” principle:

- Each folder with a README.md (kind: domain or leaf) must contain structured, machine‑parseable sections:
  1) Purpose (≤ 2 lines)
  2) In‑scope / Out‑of‑scope (bullets)
  3) Workflows (copy/pasteable nexoscli commands with flags)
  4) Rules (bullet list of folder‑specific constraints and naming policies)
  5) Checks & Gates (nexoscli dev repo check / doctor CI commands)
  6) Interfaces (Inputs/Outputs)
  7) AUTO blocks with stable ids (e.g., plans-index, children) where indexes are warranted

- These sections are co‑located in the folder’s README.md under the readme.v0 schema. The README becomes the single source of truth for local operational behavior and enforcement hints.

- Indexes (e.g., lists of plans in lifecycle folders) are represented via AUTO blocks for deterministic future refresh by CLI (readme build). Humans do not edit inside AUTO blocks.

- The repo policy (.nexoscli/readme.policy.yaml) remains the global default; folder READMEs refine and specify local constraints without violating policy or guardrails (e.g., no code under docs).

## Consequences

Pros:
- Single‑source, low‑drift documentation that is local and discoverable
- Clear structure that is machine‑readable for future `readme verify/build` tooling
- Faster onboarding and review, reduced “where is the rule?” friction
- Supports CI gates by having rules co‑located with the managed surface

Cons:
- Some duplication between high‑level docs and local READMEs
- Requires discipline to keep sections concise and current (mitigated by SLOs and CI checks)

## Scope

- Applies to all domain/leaf READMEs across the repository (docs and non‑docs paths), while respecting guardrails:
  - No executable code files under docs/
  - Protected patterns remain untouched (see .clinerules/protected-patterns.md)
  - No .readme.yaml under docs/
- READMEs under docs/project/* have been updated to include co‑located rules/workflows with a plans index where applicable.

## Implementation

- Templates:
  - Domain and leaf templates include headings: Purpose, In‑scope, Out‑of‑scope, Workflows, Rules, Checks & Gates, Interfaces, plus AUTO blocks as needed.
- docs/project folders (next/current/done/archived/backlog/paused/dumped) now contain:
  - Structured sections described above
  - Plans Index AUTO block (id=plans-index), populated where appropriate
- Policy/docs tie‑in:
  - readme.v0 schema enables deterministic verification
  - .nexoscli/readme.policy.yaml governs defaults and lint rules
  - Co‑located rules amplify policy by defining folder‑level constraints

## Enforcement

- Near‑term:
  - Use `nexoscli dev repo check --fix-readmes --output json` and `nexoscli dev doctor --ci --checks repo --output json` for structure and policy checks.
- Future (follow‑up work):
  - `nexoscli dev readme verify` to validate presence of required sections and graph links
  - `nexoscli dev readme build` to refresh AUTO blocks (e.g., plans-index)
  - CI gate extensions to assert:
    - Single file under docs/project/current/
    - Plan naming patterns in docs/project/next/
    - Immutability constraints in docs/project/done/

## Migration

- New folders should adopt the readme.v0 schema and include the structured sections upon creation.
- Existing folders should be upgraded incrementally, keeping prose minimal and adding the required sections and AUTO placeholders.
- Avoid adding .readme.yaml in docs/; prefer front‑matter + policy.

## Alternatives Considered

- Centralized rule registry: rejected (drift and index overfitting)
- Rules scattered across ADRs only: rejected (discoverability issues and mismatch with local operations)
- Fully generated READMEs: rejected (brittle diffs; harms local ownership and human clarity)

## Status

- Accepted and partially implemented. docs/project subfolders updated; repo checks green.
- Rolling adoption recommended for other areas (e.g., plugins/* domains and leafs) as changes are made there.

## References

- ADR-20250907-readme-system-v0 (README System v0)
- .nexoscli/readme.policy.yaml (policy defaults)
- .clinerules/compiled.md (term run wrapper)  
- .clinerules/protected-patterns.md (guardrails)  
- `nexoscli dev repo check`, `nexoscli dev doctor --ci --checks repo`
