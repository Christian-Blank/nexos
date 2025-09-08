---
plan_id: "59"
slug: "migrate-demo-plugin"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","derive","tests","docs"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "low"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑59 — Migrate demo plugin to Plugin Blueprint v1

Mission: Convert the demo plugin to the new modular structure with dual‑derive Args, ToolSpecs, tests, and README automation. This establishes the reference pattern for subsequent migrations.

## Outcomes (acceptance criteria)

1. Structure:
   - Create `plugins/demo/src/cmd_hello.rs` with:
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct HelloArgs { name: Option<String> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct HelloResult { message: String }`
     - `pub fn subcommand() -> clap::Command` built via `HelloArgs::augment_args(Command::new("hello").about("Say hello from demo plugin"))`
     - `pub fn run(m: &clap::ArgMatches, ctx: &CommandContext) -> Result<i32>` using `HelloArgs::from_arg_matches(m)`, emitting `ctx.ok("demo","hello", ...)`.
   - Update `plugins/demo/src/lib.rs` to be a thin router importing `cmd_hello`; route and call `cmd_hello::run`.
2. Parsing and derives:
   - No imperative `ArgMatches` parsing in `cmd_hello.rs`.
   - Args parsed via `FromArgMatches`; `augment_args` used in `subcommand()`.
3. ToolSpecs:
   - Ensure `tool_specs()` includes `demo.hello` with `args_schema` and `result_schema`.
4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/demo/README.md` AUTO (id=commands); ensure presence.
   - Catalog parity (`--check-parity`) green; no unintended toolspec name changes.
5. Tests:
   - Add `plugins/demo/tests/smoke.rs` with a minimal test invoking `demo hello` via the binary and asserting envelope shape (JSON).
6. Lists:
   - Add `plugins/demo/src/cmd_hello.rs` path to `.nexoscli/migrated/args.txt`.
   - Ensure it is not present in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changing user‑visible behavior beyond modularization and derives.
- Adding extra commands.

## Work plan

1) Create `src/cmd_hello.rs` with Args/Result, `subcommand()`, and `run()`.
2) Wire `src/lib.rs` to import and route to `cmd_hello` and include ToolSpec.
3) Add `tests/smoke.rs` that runs `nexoscli demo hello --output json` and asserts `data.message`.
4) Run exporters and update README AUTO (id=commands).
5) Update lists (`.nexoscli/migrated/args.txt`) and verify gates locally:
   ```
   nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
   nexoscli dev plugins check --strict
   nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt
   cargo test --workspace
   ```

## Must‑pass invariants (executable)

must_pass:
  - name: "no-argmatches-in-cmd"
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/demo/src/cmd_hello.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/demo/src/cmd_hello.rs | wc -l'
    expect: "== 1"

  - name: "toolspec-parity"
    cmd: |
      nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/demo/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/demo/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/demo/src/**"
    - "plugins/demo/tests/**"
    - "plugins/demo/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/demo/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with other plugin migrations (Plans 60–69).
- Does not block other plans directly; contributes to PLAN‑70 “repo hardening” completion.

## Rollback

- Revert module split and lib.rs routing adjustments; remove test. No on‑disk format change; user-visible behavior unchanged.
