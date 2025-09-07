# Rules for human

## RULE-<slug> — <short imperative rule>



# Intent
Explain *why* this rule exists in 2–4 sentences.

# Rule
The precise instruction as a checklist:
- [ ] Step 1 …
- [ ] Step 2 …

# Enforcement
- Gate: dev doctor / rules lint / CI
- Evidence: events / coverage

# Exceptions
When it's acceptable to bypass (and what to do then).

## RULE-analysis-entryspec — Use EntrySpec for Analyses



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

## RULE-cline-import — Keep Cline tasks mirrored


- Nightly (or after major efforts): `nexoscli cline import --since last`.
- Ensure `dev doctor --ci --checks cline` is green; if not, mount storage or set `NEXOS_CLINE_STORAGE`.

## RULE-post-effort-rule-sweep — After each effort, run a Rule Sweep


**Run**: `nexoscli dev telemetry report --since 7d` and `nexoscli dev doctor --ci`.  
If recurring errors or friction appear, add/change/deprecate a rule via `rules/templates/rule.md`.  
Open a proposal in `rules/proposals/CP-YYYYMMDD-*.md` and link acceptance/evidence.

