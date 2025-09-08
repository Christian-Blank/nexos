---
plan_id: "70"
slug: "repo-wide-hardening-and-finalize"
status: "proposed"
owner: "@cli/dri"
labels: ["ci-gates","blueprint-v1","cleanup","docs","tests","parity"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "low"
blast_radius: "internal-only"
depends_on: ["58","59","60","61","62","63","64","65","66","67","68","69"]
---

# PLAN‑70 — Repository‑wide hardening and finalize (Blueprint v1 complete)

Mission: Remove any remaining drift and transitional allowances, enforce Blueprint v1 invariants repo‑wide, and finalize the migration effort at 100% (no imperative parsing in migrated code, all plugins modular, READMEs and catalog consistent, CI green).

## Outcomes (acceptance criteria)

1. Allow‑list empty; args‑enforce repo‑wide:
   - `.nexoscli/allow/argmatches.txt` is empty (or deleted).
   - `nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt` passes with no violations.
   - Optional: promote `--migrated` to cover all `plugins/**/src/cmd_*.rs` (i.e., all migrated).
2. Plugins structure complete:
   - `nexoscli dev plugins check --strict` passes across all plugins:
     - README present and contains AUTO (id=commands).
     - `tests/` present with ≥1 test file.
     - No `src/*.rs` exceeds 2000 LOC; warn threshold respected (≤ 1000 LOC).
3. Docs/catalogn determinism:
   - `docs/CLI_REFERENCE.md` and `docs/catalog/commands.json` regenerate without drift; CI gate passes.
   - `commands.json` validates against JSON Schema (existing CI step).
   - Parity (`--check-parity`) passes.
4. ToolSpecs correctness:
   - All intended agent‑grade commands expose ToolSpecs; non‑agent commands intentionally have toolspec=null.
   - No orphan ToolSpecs (none exported without catalog linkage).
5. Documentation refresh:
   - Update `docs/architecture/plugin-blueprint-v1.md` `last_reviewed` to current date; add a short section “Blueprint v1 migration complete”.
6. Visibility and hygiene:
   - Add a short `docs/WORKFLOWS_PLUGIN.md` note (or update) indicating Blueprint v1 is in place; reference plans 58–70.
   - Ensure CI profile v0 in `dev doctor --ci` includes plugins/tools/repo/plans/doc gates.

## Non‑goals

- Introducing new commands or contracts; this plan only hardens and finalizes the migration.

## Work plan

1) Remove transitional allowances:
   - Empty `.nexoscli/allow/argmatches.txt`.
   - Ensure `.nexoscli/migrated/args.txt` includes all `plugins/**/src/cmd_*.rs`.
2) CI gates:
   - Keep `docs` and `catalog` drift gates + schema validation.
   - Keep `nexoscli dev plugins check --strict`.
   - Keep `nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt`.
3) Docs:
   - Update `docs/architecture/plugin-blueprint-v1.md` (`last_reviewed`) and add completion note.
   - Ensure each plugin README has AUTO (id=commands) after a regen.
4) Final verification (local and CI):
   ```
   nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
   nexoscli dev plugins check --strict
   nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt
   nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity
   cargo test --workspace
   ```

## Must‑pass invariants (executable)

must_pass:
  - name: "allow-list-empty"
    cmd: 'test ! -s .nexoscli/allow/argmatches.txt'
    expect_exit: 0

  - name: "args-enforce-all-green"
    cmd: 'nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --output json'
    expect_exit: 0

  - name: "plugins-structure-green"
    cmd: 'nexoscli dev plugins check --strict --output json'
    expect_exit: 0

  - name: "catalog-parity-and-schema"
    cmd: |
      nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity
      python - <<'PY'
      import json
      from jsonschema import validate, Draft7Validator
      with open('docs/catalog/commands.json') as f: data=json.load(f)
      with open('docs/catalog/schema/commands_catalog.schema.json') as f: schema=json.load(f)
      Draft7Validator.check_schema(schema); validate(instance=data, schema=schema)
      print("OK")
      PY
    expect_exit: 0

  - name: "docs-and-catalog-no-drift"
    cmd: |
      nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
      git add -A && git diff --cached --quiet
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - ".nexoscli/**"
    - ".github/workflows/**"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - "docs/architecture/plugin-blueprint-v1.md"
    - "plugins/*/README.md"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("

## Parallelization and dependencies

- depends_on: ["58","59","60","61","62","63","64","65","66","67","68","69"] — all migrations complete.
- Not parallelizable with those migrations (must run after); can be executed as the final consolidation.
- Blocks: none; marks end of Blueprint v1 rollout.

## Rollback

- Re‑add allow‑list entries and loosen CI gates if needed; does not impact user‑visible behavior.
