---
id: RULE-analysis-entryspec
title: "Use EntrySpec for Analyses"
audience: human
scope: global
status: Active
version: 0.1.0
last_reviewed: 2025-09-07T00:00:00Z
principle: "Determinism, Reproducibility"
owner: "rules-dri@example.com"
---

# Intent
All repo‑level analyses must be deterministic and reproducible. A single‑file EntrySpec drives uniform outputs, reduces entropy, and enables agent runs that are safe, idempotent, and navigable via README graphs.

# Rule
- [ ] Create a single file at `docs/analysis/<yyyy-mm>_<slug>/ENTRY.md` using `analysis.entry.v0` front‑matter with an explicit `write_allowlist`.
- [ ] Produce standard outputs: `00-methodology.md`, `01-inventory.md`, `02-systems-and-relations.md`, `03-cli-capabilities.md`, `04-findings-gaps-recos.md`; each includes front‑matter and correct `parents`/`children`.
- [ ] In `03-cli-capabilities.md`, label each claim as Confirmed (tests/code) or Inferred (docs/structure).
- [ ] Execute via `docs/prompts/cline/execute_entry.workflow.md`, respecting `inputs.include`/`exclude`, `read_order`, `write_allowlist`, and acceptance checks.
- [ ] Keep changes docs‑only unless generator/CLI support exists; prefer generators later.

# Enforcement
- Gate: `dev doctor --ci`, `rules lint`, CI.
- Evidence: EntrySpec present with required front‑matter; outputs exist with graph links; repo check and doctor OK.

# Exceptions
- Documentation/analysis tasks only. If deviating (e.g., code generators/CLI changes), open a change proposal (CP/ADR) and link acceptance/evidence.
