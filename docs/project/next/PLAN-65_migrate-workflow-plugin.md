---
plan_id: "65"
slug: "migrate-workflow-plugin"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","derive","tests","docs","agents"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "medium"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑65 — Migrate workflow plugin to Plugin Blueprint v1

Mission: Convert the workflow plugin to modular `cmd_*` structure with dual‑derive Args/Result, ToolSpecs alignment, tests, and README AUTO updates. Preserve current agent‑grade ToolSpecs.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - Create modules under `plugins/workflow/src/`:
     - `cmd_init.rs`, `cmd_list.rs`, `cmd_plan.rs`, `cmd_query.rs`, `cmd_run.rs`, `cmd_signal.rs`, `cmd_status.rs`, `cmd_validate.rs`, `cmd_upsert_sa.rs`
   - Each module defines:
     - Args struct: `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)]`
     - Result struct mirroring current catalog schemas (where applicable)
     - `subcommand()` built via `<Args>::augment_args(...)`
     - `run(m, ctx)` using `<Args>::from_arg_matches(m)` and emitting `ctx.ok(...)`
   - Update `plugins/workflow/src/lib.rs` to be a thin router importing modules and routing to `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` in `cmd_*` modules.
   - Typed parsing via `FromArgMatches`; flags via `Args::augment_args`.

3. ToolSpecs:
   - Ensure `tool_specs()` includes:
     - `workflow.init`, `workflow.list`, `workflow.plan`, `workflow.query`, `workflow.run`, `workflow.signal`, `workflow.status`, `workflow.validate`, `workflow.upsert-sa`
   - Args/result schemas reflect those exported today in the catalog.

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/workflow/README.md` AUTO (id=commands).
   - Catalog parity (`--check-parity`) remains green.

5. Tests:
   - Add `plugins/workflow/tests/smoke.rs` with minimal tests for representative commands:
     - `workflow list --output json`
     - `workflow plan --spec examples/workflows/WF-parent.yaml --output json` (or a safe fixture)
     - `workflow validate --spec ... --output json` (fixture)

6. Lists:
   - Append new module paths to `.nexoscli/migrated/args.txt`.
   - Ensure none are present in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changing workflow engine behavior; focus on structure and derives.
- Adding/removing commands.

## Work plan

1) Implement the listed modules with Args/Result, `subcommand()`, `run()`.
2) Update `src/lib.rs` router and `tool_specs()`.
3) Add tests/smoke.rs for list/plan/validate happy paths.
4) Regenerate docs and README.
5) Update lists and verify gates:
   ```
   nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
   nexoscli dev plugins check --strict
   nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt
   nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity
   cargo test --workspace
   ```

## Must‑pass invariants (executable)

must_pass:
  - name: "no-argmatches-in-cmds"
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/workflow/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/workflow/src/cmd_*.rs | wc -l'
    expect: ">= 5"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/workflow/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/workflow/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/workflow/src/**"
    - "plugins/workflow/tests/**"
    - "plugins/workflow/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/workflow/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59–64 and 66–69.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs updates; remove tests. No on‑disk format change or user-visible CLI changes required.
