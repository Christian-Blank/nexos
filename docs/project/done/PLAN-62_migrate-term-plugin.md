---
plan_id: '62'
slug: migrate-term-plugin
status: done
owner: '@cli/dri'
labels:
- plugins
- blueprint-v1
- derive
- tests
- docs
created_at: 2025-09-08T00:00:00Z
source: next
risk: low
blast_radius: internal-only
depends_on:
- '58'
completed_at: 2025-09-09T04:56:04.847563Z
implemented_in: 45c3e5e
---
# PLAN‑62 — Migrate term plugin to Plugin Blueprint v1

Mission: Convert the term plugin to modular `cmd_*` structure with dual‑derive Args, tests, and README AUTO updates. Decide agent‑exposure (ToolSpec) for `term run` explicitly to avoid accidental complexity in catalog.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - `plugins/term/src/cmd_run.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct TermRunArgs {`
       - `#[arg(long)] pub cwd: Option<String>,`
       - `#[arg(long = "fs-exclude")] pub fs_exclude: Option<String>,`
       - `#[arg(long = "fs-hash")] pub fs_hash: bool,`
       - `#[arg(long = "fs-root")] pub fs_root: Option<String>,`
       - `#[arg(long = "fs-topic")] pub fs_topic: bool,`
       - `#[arg(long)] pub passthrough: bool,`
       - `#[arg(long = "sample-kb")] pub sample_kb: Option<usize>,`
       - `#[arg(required = true)] pub command: String`
       - `/* normalize any positional capture from current behavior */`
       - `}`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct TermRunResult { /* summarize execution, captured outputs, digests if any */ }`
     - `subcommand()` via `TermRunArgs::augment_args(Command::new("run").about("Execute a command and capture output"))`
     - `run()` parses via `TermRunArgs::from_arg_matches(m)`; emits `ctx.ok("term","run", ...)`
   - Update `plugins/term/src/lib.rs` to be a thin router importing `cmd_run` and routing to `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` parsing in `cmd_run.rs`.
   - Flags synthesized via `Args::augment_args`; typed parsing via `FromArgMatches`.

3. ToolSpec decision (explicit):
   - Keep `term.run` as non‑agent (toolspec=null) for now (infra command; high variance and side‑effects). Document decision in code comments and plan notes.
   - If later needed for agents, add ToolSpec in a dedicated plan.

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/term/README.md` AUTO (id=commands).
   - Catalog retains `term run` with `toolspec: null`. No parity issues (ToolSpec not required).

5. Tests:
   - Add `plugins/term/tests/smoke.rs` with a dry test (invoke with a harmless command or mock mode if available) to assert JSON envelope structure. If execution is unsafe in CI, assert help/usage path instead.

6. Lists:
   - Append new module path to `.nexoscli/migrated/args.txt`.
   - Ensure it is not present in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changing the core execution semantics of `term run` (keep behavior; surface the typed args).
- Adding ToolSpec for agents in this plan.

## Work plan

1) Implement `cmd_run.rs` with Args/Result, `subcommand()`, and `run()`.
2) Update `src/lib.rs` to import and route to `cmd_run`.
3) Add a minimal `tests/smoke.rs` (safe invocation or usage assertion).
4) Regenerate docs and update plugin README.
5) Update lists and verify gates:
   ```
   nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
   nexoscli dev plugins check --strict
   nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt
   cargo test --workspace
   ```

## Must‑pass invariants (executable)

must_pass:
  - name: "no-argmatches-in-cmd"
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/term/src/cmd_run.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/term/src/cmd_run.rs | wc -l'
    expect: "== 1"

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/term/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/term/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/term/src/**"
    - "plugins/term/tests/**"
    - "plugins/term/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/term/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59–61 and 63–69.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs routing changes; remove tests. No on‑disk format change or user-visible behavior changes required.
