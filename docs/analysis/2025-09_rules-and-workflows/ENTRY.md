---
schema: analysis.entry.v0
title: "Rules & Workflows — EntrySpec (2025‑09)"
summary: "Deterministic single-file entry to generate/update the R&W analysis set."
owner: "@rules-workflows/dri"
status: "draft"
version: "0.1.0"
created_at: "2025-09-07T00:00:00Z"
parents: ["./README.md","../README.md","../../README.md"]
topic_slug: "2025-09_rules-and-workflows"
inputs:
  include:
    - "../../**/*.md"
    - "../../../rules/**"
    - "../../../examples/workflows/**"
    - "../../../plugins/**"
    - "../../../nexoscli-bin/tests/**/*.rs"
  exclude:
    - "../../../target/**"
    - "../../../**/node_modules/**"
    - "../../../**/.nexoscli/**"
  read_order:
    - "../../RULES.md"
    - "../../WORKFLOWS.md"
    - "../../../rules/**"
    - "../../../examples/workflows/**"
    - "../../prompts/cline/**"
    - "../../../plugins/rules/**"
    - "../../../plugins/workflow/**"
    - "../../ADRs/**"
outputs:
  - path: "./00-methodology.md"
    kind: "md"
  - path: "./01-inventory.md"
    kind: "md"
  - path: "./02-systems-and-relations.md"
    kind: "md"
  - path: "./03-cli-capabilities.md"
    kind: "md"
  - path: "./04-findings-gaps-recos.md"
    kind: "md"
write_allowlist:
  - "./*.md"
acceptance:
  - "All 5 outputs exist and render without broken relative links."
  - "Every file includes correct front-matter + parents/children."
  - "03-cli-capabilities.md clearly labels Confirmed vs Inferred."
  - "04-findings-gaps-recos.md has 3–5 prioritized recommendations with gates."
commit:
  message: "analysis(rules-workflows): v0 deterministic report"
  co_authors: []
notes:
  - "Prefer tests in nexoscli-bin/tests/** to confirm CLI claims."
  - "Do not redefine sources of truth; link to them."
---

# Agent Protocol (execute verbatim)

1) **Load this EntrySpec** and confirm paths are relative to `docs/analysis/2025-09_rules-and-workflows/`.  
2) **Discover inputs** using `inputs.include` − `inputs.exclude`, following `read_order`. Do not read outside this set.  
3) **Generate/refresh outputs** listed under `outputs` using the structures defined in `_templates`:
   - `00-methodology.md` — deterministic method used here (brief).
   - `01-inventory.md` — enumerated list of rules/workflows/prompts/plugins/tests (one‑liners, why they matter).
   - `02-systems-and-relations.md` — bounded contexts + a mermaid diagram + SSoT matrix.
   - `03-cli-capabilities.md` — per‑plugin capability audit with **Confirmed** vs **Inferred**.
   - `04-findings-gaps-recos.md` — concise, prioritized actions with acceptance gates.
4) **Front‑matter & graph:** Ensure each file has front‑matter and correct `parents`/`children` so the README graph is navigable.  
5) **Write policy:** Only write within `write_allowlist`. Update files idempotently.  
6) **Acceptance check:** Verify all acceptance criteria in `acceptance`.  
7) **Summary output:** Print changed files and any TODOs for future confirmation.

(Optional) If you want a README.md inside the example folder, you can copy docs/analysis/_templates/README.md and adjust the title/summary.
