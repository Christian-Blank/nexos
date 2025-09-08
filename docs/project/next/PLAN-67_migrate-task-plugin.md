---
plan_id: "67"
slug: "migrate-task-plugin"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","derive","tests","docs","infra"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "medium"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑67 — Migrate task plugin to Plugin Blueprint v1

Mission: Convert the task plugin to modular `cmd_*` structure with dual‑derive Args/Result, tests, and README AUTO updates. Make an explicit decision about agent exposure (ToolSpecs) for task lifecycle commands to avoid accidental complexity.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - `plugins/task/src/cmd_new.rs` — `task new`
   - `plugins/task/src/cmd_start.rs` — `task start`
   - `plugins/task/src/cmd_end.rs` — `task end`
   - `plugins/task/src/cmd_pause.rs` — `task pause`
   - `plugins/task/src/cmd_resume.rs` — `task resume`
   - `plugins/task/src/cmd_stage.rs` — `task stage`
   - `plugins/task/src/cmd_status.rs` — `task status`
   - `plugins/task/src/cmd_list.rs` — `task list`
   - `plugins/task/src/cmd_bundle.rs` — `task bundle`
   - Each module defines:
     - Args struct: `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)]`
     - Result struct reflecting current envelopes (where applicable)
     - `subcommand()` via `<Args>::augment_args(...)`
     - `run(m, ctx)` using `<Args>::from_arg_matches(m)` and emitting `ctx.ok(...)`
   - Update `plugins/task/src/lib.rs` to be a thin router importing modules and routing to their `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` in new `cmd_*` modules.
   - Typed parsing via `FromArgMatches`; flags via `Args::augment_args`.

3. ToolSpecs decision (explicit):
   - Treat task lifecycle commands primarily as infra; keep ToolSpecs null for now (catalog currently shows toolspec=null for most task commands).
   - If later agent‑exposure is desired for specific commands (e.g., `task new`), add ToolSpecs in dedicated follow‑up plans.

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/task/README.md` AUTO (id=commands).
   - Catalog parity remains green; no unintentional ToolSpec additions.

5. Tests:
   - Add `plugins/task/tests/smoke.rs` with minimal tests invoking:
     - `task new --title "X" --output json` (dry path where possible), assert envelope shape.
     - `task status --output json` (and/or `task list`), assert envelope shape.

6. Lists:
   - Append new module paths to `.nexoscli/migrated/args.txt`.
   - Ensure none are present in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changing task lifecycle semantics or task store; structural migration only.
- Adding ToolSpecs in this plan.

## Work plan

1) Implement modules with Args/Result, `subcommand()`, `run()` per command.
2) Update `src/lib.rs` to import modules and route to `run()`.
3) Add tests/smoke.rs for representative commands (new/status/list).
4) Regenerate docs and README; verify catalog unchanged in intent.
5) Update lists and verify gates:
   ```
   nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
   nexoscli dev plugins check --strict
   nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt
   cargo test --workspace
   ```

## Must‑pass invariants (executable)

must_pass:
  - name: "no-argmatches-in-cmds"
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/task/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/task/src/cmd_*.rs | wc -l'
    expect: ">= 5"

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/task/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/task/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/task/src/**"
    - "plugins/task/tests/**"
    - "plugins/task/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/task/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59–66 and 68–69.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs router changes; remove tests. No on‑disk format change or user-visible CLI changes required.
