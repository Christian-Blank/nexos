---
schema: analysis.entry.v0
title: "<Topic> — EntrySpec"
summary: "Single-file, deterministic instructions for an analysis/refactor effort."
owner: "<@dri>"
status: "template"
version: "0.1.0"
created_at: "YYYY-MM-DDTHH:MM:SSZ"
parents: ["../README.md","../../README.md"]
topic_slug: "<yyyy-mm>_<topic-slug>"
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
    - "../../../plugins/**"
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
  - "All outputs exist and render without broken relative links."
  - "Every file includes correct front-matter + parents/children."
  - "03-cli-capabilities labels Confirmed vs Inferred."
  - "04-findings-gaps-recos has 3–5 prioritized, testable gates."
commit:
  message: "analysis(<topic-slug>): initial pass v0"
  co_authors: []
notes:
  - "Prefer confirmed facts from tests/code; mark inferences."
---

# Agent Protocol (execute verbatim)

1) **Load this EntrySpec.** Treat front‑matter as the authoritative contract.  
2) **Gather context** only from `inputs.include` − `inputs.exclude` in `read_order`.  
3) **Produce outputs** exactly as listed under `outputs`, with these constraints:
   - Each file starts with front‑matter: `schema`, `title`, `owner`, `status`, `version`, `parents`, `children`.
   - Use ≤ 3 heading levels; short paragraphs; prefer bullets and tables.
   - In `03-cli-capabilities.md`, label each statement **Confirmed** (tests/code) or **Inferred** (docs/structure).
4) **Write policy:** Only write paths matching `write_allowlist`. Update idempotently.  
5) **Acceptance check:** Ensure all `acceptance` points pass. If any fail, stop and report which and why.  
6) **Summary output:** At the end, print changed files, notable inferences (to confirm later), and the minimal follow‑ups.  
7) **(Optional) Commit:** Propose `commit.message` for the human to apply.
