---
plan_id: "66"
slug: "migrate-replay-plugin"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","derive","tests","docs","agents","large"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "medium"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑66 — Migrate replay plugin to Plugin Blueprint v1

Mission: Convert the replay plugin to modular `cmd_*` structure with dual‑derive Args/Result, ToolSpecs alignment per catalog, tests, and README AUTO updates. This is the largest migration and will be split across feature groups to keep LOC and review sizes manageable.

## Outcomes (acceptance criteria)

1. Structure (split into modules by feature group):
   - `plugins/replay/src/cmd_annotate_add.rs` — `replay annotate add`
   - `plugins/replay/src/cmd_annotate_list.rs` — `replay annotate list`
   - `plugins/replay/src/cmd_bag_build.rs` — `replay bag build`
   - `plugins/replay/src/cmd_bag_verify.rs` — `replay bag verify`
   - `plugins/replay/src/cmd_bag_info.rs` — `replay bag info`
   - `plugins/replay/src/cmd_bag_cat.rs` — `replay bag cat`
   - `plugins/replay/src/cmd_export.rs` — `replay export`
   - `plugins/replay/src/cmd_extract.rs` — `replay extract`
   - `plugins/replay/src/cmd_fileops_derive.rs` — `replay fileops derive`
   - `plugins/replay/src/cmd_normalize.rs` — `replay normalize`
   - `plugins/replay/src/cmd_plan.rs` — `replay plan`
   - `plugins/replay/src/cmd_run.rs` — `replay run`
   - `plugins/replay/src/cmd_schema_events.rs` — `replay schema events`
   - `plugins/replay/src/cmd_schema_get.rs` — `replay schema get`
   - `plugins/replay/src/cmd_schema_list.rs` — `replay schema list`
   - `plugins/replay/src/cmd_validate.rs` — `replay validate`
   - `plugins/replay/src/cmd_verify.rs` — `replay verify`
   - Each module defines:
     - Args struct: `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)]`
     - Result struct mirroring current catalog schemas (when applicable)
     - `subcommand()` built via `<Args>::augment_args(Command::new(...).about(...))`
     - `run(m, ctx)` using `<Args>::from_arg_matches(m)` and emitting `ctx.ok(...)`
   - Update `plugins/replay/src/lib.rs` to be a thin router importing only these modules and routing to their `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` usage in any new `cmd_*` modules.
   - Typed parsing via `FromArgMatches`; flags via `Args::augment_args`.

3. ToolSpecs:
   - Ensure `tool_specs()` includes (as per catalog) agent‑facing commands:
     - `replay.bag.verify`, `replay.export`, `replay.fileops.derive`, `replay.normalize`, `replay.schema.events`, `replay.schema.get`, `replay.schema.list`, `replay.validate`, `replay.verify`
   - Commands intended for local UX only may remain toolspec=null; document in code.

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/replay/README.md` AUTO (id=commands).
   - Catalog parity (`--check-parity`) stays green; ToolSpec names unchanged.

5. Tests:
   - Add `plugins/replay/tests/smoke.rs` with a few focused tests:
     - `replay bag verify --bag testdata/sample.tgz --output json` (or a small fixture) asserts envelope structure; mark as skip if fixture unavailable in CI.
     - `replay normalize --to v0.3 --output json` (simulate or dry path) asserts envelope shape.
     - `replay schema list --output json` asserts envelope.
   - Keep tests small and isolated; avoid large I/O when possible.

6. Lists:
   - Append all new `cmd_*.rs` paths to `.nexoscli/migrated/args.txt`.
   - Ensure none are in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Redesigning replay contract semantics or large I/O workflow; focus on structure and derives.
- Adding new commands.

## Work plan

1) Implement modules per list with Args/Result, `subcommand()`, `run()`, grouping PRs if needed (schema group, bag group, core ops group).
2) Update `src/lib.rs` router and `tool_specs()`.
3) Add tests/smoke.rs; add small fixtures if necessary under `testdata/`.
4) Regenerate docs and README; verify catalog parity.
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
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/replay/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/replay/src/cmd_*.rs | wc -l'
    expect: ">= 8"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/replay/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/replay/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/replay/src/**"
    - "plugins/replay/tests/**"
    - "plugins/replay/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/replay/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59–65, 67–69 (coordinate for fixtures if shared).
- Contributes heavily to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs updates; remove tests. No on‑disk format change; user-visible CLI unchanged.
