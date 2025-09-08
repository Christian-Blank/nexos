---
plan_id: "61"
slug: "migrate-rules-plugin"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","derive","tests","docs"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "low"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑61 — Migrate rules plugin to Plugin Blueprint v1

Mission: Convert the rules plugin to modular `cmd_*` structure with dual‑derive Args, ToolSpecs, tests, and README AUTO updates to align with Blueprint v1 and Plans 53–55.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - `plugins/rules/src/cmd_init.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct RulesInitArgs { #[arg(long)] pub force: bool }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct RulesInitResult { written: String }`
     - `subcommand()` via `RulesInitArgs::augment_args(Command::new("init").about("Create minimal rules pack"))`
     - `run()` uses `RulesInitArgs::from_arg_matches(m)`; emits `ctx.ok("rules","init", ...)`
     - ToolSpec for `rules.init` may be omitted (toolspec=null) if we treat it as infra; document in code.
   - `plugins/rules/src/cmd_export.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct RulesExportArgs { #[arg(long)] pub audience: String, #[arg(long)] pub out: Option<String> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct RulesExportResult { /* could be opaque (Any) or a small envelope */ }`
     - `subcommand()` via `RulesExportArgs::augment_args(Command::new("export").about("Export audience-specific bundle"))`
     - `run()` parses and emits `ctx.ok("rules","export", ...)`
     - ToolSpec present for `rules.export` per catalog
   - `plugins/rules/src/cmd_lint.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct RulesLintArgs { }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct RulesLintResult { files: u32, issues: Vec<String>, ok: bool }`
     - `subcommand()` via `RulesLintArgs::augment_args(Command::new("lint").about("Validate rules/*.md front-matter"))`
     - `run()` parses and emits `ctx.ok("rules","lint", ...)`
     - ToolSpec present for `rules.lint`
   - Update `plugins/rules/src/lib.rs` to be a thin router importing `cmd_init`, `cmd_export`, `cmd_lint`, routing to their `run()`; expose ToolSpecs accordingly.

2. Parsing and derives:
   - No imperative `ArgMatches` parsing in new `cmd_*` modules.
   - Args parsed via `FromArgMatches`; flags synthesized via `<Args>::augment_args`.

3. ToolSpecs:
   - Ensure `tool_specs()` includes:
     - `rules.export` (args/result)
     - `rules.lint` (args/result)
     - `rules.init` may remain toolspec=null (infra) unless we choose to define one (optional).

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/rules/README.md` AUTO (id=commands).
   - Catalog parity (`--check-parity`) stays green; no unintended toolspec name drift.

5. Tests:
   - Add `plugins/rules/tests/smoke.rs` with basic assertions invoking:
     - `rules lint --output json` (envelope present)
     - `rules export --audience human --output json` (envelope present)

6. Lists:
   - Append new module paths to `.nexoscli/migrated/args.txt`.
   - Ensure none of the new files are present in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changes to underlying rules pack semantics; this plan is about structure and derives.
- Adding new commands.

## Work plan

1) Implement `cmd_init.rs`, `cmd_export.rs`, `cmd_lint.rs` with Args/Result, `subcommand()`, `run()`.
2) Update `src/lib.rs` router and `tool_specs()`.
3) Add tests/smoke.rs for lint and export happy paths.
4) Regenerate docs and update README.
5) Update lists and verify local gates:
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
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/rules/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/rules/src/cmd_*.rs | wc -l'
    expect: ">= 2"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/rules/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/rules/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/rules/src/**"
    - "plugins/rules/tests/**"
    - "plugins/rules/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/rules/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59, 60, 62–69.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs updates; remove tests. No on‑disk format change or user-visible CLI changes required.
