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

## RULE-cline-import — Keep Cline tasks mirrored


- Nightly (or after major efforts): `nexoscli cline import --since last`.
- Ensure `dev doctor --ci --checks cline` is green; if not, mount storage or set `NEXOS_CLINE_STORAGE`.

## RULE-post-effort-rule-sweep — After each effort, run a Rule Sweep


**Run**: `nexoscli dev telemetry report --since 7d` and `nexoscli dev doctor --ci`.  
If recurring errors or friction appear, add/change/deprecate a rule via `rules/templates/rule.md`.  
Open a proposal in `rules/proposals/CP-YYYYMMDD-*.md` and link acceptance/evidence.

