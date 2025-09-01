# Engineering Playbook (v0)

## Working Agreements

- **WIP**: 1 feature in progress, 1 meta change in review.
- **Rule of 3**: third repeat → automate (generator); risky multi-file edits → automate now.
- **Tighten-only**: children may narrow budgets/constraints; widening requires CP→ADR.
- **Small PRs**: ≤ ~300 LOC or ≤ 6 files.
- **Default to action**: stuck > 10 min → open an atomic_issue and move.

## Double Learning Loop (after each milestone)

**L1 — Feature Review (≤15 min):** value shipped, evidence (spans/events/demo), 3 frictions → issues, pick next feature.  
**L2 — Meta Review (≤10 min, only if triggered):** CP if (1) blockage, (2) repeats across ≥2 features, or (3) violates a budget. If accepted → ADR.

## Definitions

**Ready:** problem ≤ 3 sentences; DRI named; deps listed; ≤ 8 binary acceptance criteria; added to exec_graph.  
**Done:** E2E passes; JSON outputs stable; event written; spans present; brief README snippet updated if surface changed; short demo/evidence linked.

## CI Validation

**Origin validation** runs before any changes to ensure structural integrity:

```bash
# Install CI dependencies (once)
uv venv scripts/ci/.venv
uv pip install --python scripts/ci/.venv/bin/python pyyaml

# Run validation
scripts/ci/.venv/bin/python scripts/ci/validate_origin.py
```

Must pass before any commits to main. Validates required origin documents and exec_graph structure.

## Folder Rule

Any bounded context (folder) must have a one-page README with project_brief front-matter and a link to its parent. Child NS optional; if present, it only tightens budgets.
