---
plan_id: "58"
slug: "repo-structure-gates-and-enforcement"
status: "proposed"
owner: "@cli/dri"
labels: ["ci-gates","rules","docs","blueprint-v1","tests"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "low"
blast_radius: "internal-only"
depends_on: ["55"]
---

# PLAN‑58 — Repo structure gates and enforcement (Blueprint v1 enabling)

Mission: Establish baseline, repo‑wide guardrails so all plugin migrations can proceed in parallel, without backsliding. Make structure, derives, docs, and tests observable, enforceable, and green in CI.

## Outcomes (acceptance criteria)

1. Lists for incremental enforcement:
   - `.nexoscli/migrated/args.txt` — relative file paths (plugins/*/src/cmd_*.rs) that must obey dual‑derive and FromArgMatches parsing.
   - `.nexoscli/allow/argmatches.txt` — relative file paths temporarily allowed to use imperative `ArgMatches` parsing until migrated.
2. CI gates updated:
   - Run `nexoscli dev plugins check --strict` to enforce:
     - Each plugin has `README.md` and `tests/` directory (≥1 .rs).
     - LOC thresholds per `src/*.rs` (warn ≥ 1000 LOC; error ≥ 2000 LOC).
     - Command README has AUTO (id=commands) block.
   - Run `nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt`; fail if:
     - Any migrated file lacks `#[derive(clap::Args, schemars::JsonSchema, Serialize, Deserialize)]`.
     - Any non‑allow‑listed file uses `get_one::<` or `get_flag(` imperative parsing.
   - Keep existing docs drift gate green for:
     - `docs/CLI_REFERENCE.md` and `docs/catalog/commands.json` (already in CI).
     - Continue validating `commands.json` JSON Schema.
3. Deterministic READMEs for all plugins:
   - CI step runs `nexoscli dev docs commands --apply-readmes` so all plugin `README.md` contain AUTO (id=commands).
4. Documentation:
   - Update `docs/architecture/plugin-blueprint-v1.md` last_reviewed to current date in follow‑up PR (out of scope for this plan if drift‑gated).

## Non‑goals

- Migrating any specific plugin command to dual‑derive (covered by subsequent plans).
- Adding/removing ToolSpecs (only gates and lists here).

## Work plan

1) Lists
- Add two files to repo root:
  - `.nexoscli/migrated/args.txt` (initially: `plugins/dev/src/cmd_init.rs`).
  - `.nexoscli/allow/argmatches.txt` (initially: empty or pre‑populated with known non‑migrated hot paths if needed).

2) CI updates
- Ensure the pipeline includes (in order):
  - `cargo test --workspace`
  - Docs drift + catalog build, schema validation (already present)
  - `nexoscli dev docs commands --apply-readmes` then `git diff --cached --quiet`
  - `nexoscli dev plugins check --strict`
  - `nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt`

3) Local runbook (devs)
- On migrating a file to dual‑derive:
  - Append its path to `.nexoscli/migrated/args.txt`.
  - Remove it from `.nexoscli/allow/argmatches.txt` if present.
- Run:
  ```
  nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
  nexoscli dev plugins check --strict
  nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt
  ```

## Must‑pass invariants (executable)

must_pass:
  - name: "lists-present"
    cmd: 'test -f .nexoscli/migrated/args.txt && test -f .nexoscli/allow/argmatches.txt'
    expect_exit: 0

  - name: "plugins-structure-green"
    cmd: 'nexoscli dev plugins check --strict --output json'
    expect_exit: 0

  - name: "args-enforce-migrated-green"
    cmd: 'nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt --output json'
    expect_exit: 0

  - name: "readmes-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/*/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: ">= 1"

  - name: "catalog-and-docs-no-drift"
    cmd: |
      nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
      git add -A && git diff --cached --quiet
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - ".nexoscli/**"
    - ".github/workflows/**"
    - "plugins/*/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  notes:
    - This plan intentionally does not modify src/*.rs beyond list references.

## Parallelization and dependencies

- depends_on: ["55"] (PLAN‑55 is done; gates rely on its exporters).
- Unblocks: PLAN‑59..PLAN‑69 (all plugin migrations can proceed in parallel once gates are in place).
- Blocks: None of the migrations strictly, but recommended to land first to prevent regressions.

## Rollback

- Remove CI gate steps and the two list files. No user‑visible changes. Deterministic doc steps can remain.
