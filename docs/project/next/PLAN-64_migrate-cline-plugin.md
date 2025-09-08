---
plan_id: "64"
slug: "migrate-cline-plugin"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","derive","tests","docs","agents"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "low"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑64 — Migrate cline plugin to Plugin Blueprint v1

Mission: Convert the cline plugin to modular `cmd_*` structure with dual‑derive Args/Result, ToolSpecs alignment (keep current scope), tests, and README AUTO updates.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - `plugins/cline/src/cmd_locate.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct LocateArgs { #[arg(long)] pub dry_run: bool, #[arg(long)] pub limit: Option<u64>, #[arg(long)] pub since: Option<String>, #[arg(long)] pub storage: Option<String> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct LocateResult { selected: Option<String>, candidates: Vec<serde_json::Value>, hints: Vec<String> } /* mirror catalog schema */`
     - `subcommand()` via `LocateArgs::augment_args(Command::new("locate").about("Find Cline storage (globalStorage/workspace)"))`
     - `run()` parses via `LocateArgs::from_arg_matches(m)`
   - `plugins/cline/src/cmd_import.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct ImportArgs { #[arg(long)] pub dry_run: bool, #[arg(long)] pub limit: Option<u64>, #[arg(long)] pub since: Option<String>, #[arg(long)] pub storage: Option<String> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct ImportSummary { /* per catalog */ }`
     - `subcommand()` via `ImportArgs::augment_args(Command::new("import").about("Import tasks (read-only)"))`
     - `run()` parses via `ImportArgs::from_arg_matches(m)`
   - `plugins/cline/src/cmd_correlate.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct CorrelateArgs { #[arg(long)] pub dry_run: bool, #[arg(long, default_value="5m")] pub pad: String, #[arg(long)] pub run_id: Option<String>, #[arg(long)] pub since: Option<String>, #[arg(long)] pub write: bool }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct CorrelateResult { /* free-form (Any) acceptable if unchanged */ }`
     - `subcommand()` via `CorrelateArgs::augment_args(Command::new("correlate").about("Correlate Cline tasks to runs"))`
     - `run()` parses via `CorrelateArgs::from_arg_matches(m)`
   - `plugins/cline/src/cmd_normalize.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct NormalizeArgs { #[arg(long)] pub overwrite: bool, #[arg(long)] pub since: Option<String>, #[arg(long = "task-id")] pub task_id: Option<String> }`
     - Result may remain implicit (toolspec=null) per catalog.
     - `subcommand()` via `NormalizeArgs::augment_args(Command::new("normalize").about("Normalize Cline tasks → AIM streams"))`
     - `run()` parses via `NormalizeArgs::from_arg_matches(m)`
   - Update `plugins/cline/src/lib.rs` to be a thin router importing the new modules; route to their `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` usage in new `cmd_*` modules.
   - Flags via `<Args>::augment_args`; typed parsing via `FromArgMatches`.

3. ToolSpecs:
   - Ensure `tool_specs()` includes (as per catalog):
     - `cline.locate`, `cline.import`, `cline.correlate` (args/result schemas).
   - Keep `cline.normalize` as toolspec=null for now (not in catalog); document decision.

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/cline/README.md` AUTO (id=commands).
   - Parity (`--check-parity`) remains green (names unchanged).

5. Tests:
   - Add `plugins/cline/tests/smoke.rs` with minimal tests invoking:
     - `cline locate --output json`
     - `cline import --dry-run --output json` (safe path)
     - Optionally `cline correlate --dry-run --pad 5m --output json`

6. Lists:
   - Append new module paths to `.nexoscli/migrated/args.txt`.
   - Ensure they are not in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changing cline storage discovery heuristics or I/O semantics (structure only).
- Adding a ToolSpec for `normalize` in this plan.

## Work plan

1) Implement `cmd_locate.rs`, `cmd_import.rs`, `cmd_correlate.rs`, `cmd_normalize.rs`.
2) Update `src/lib.rs` router and `tool_specs()`.
3) Add tests/smoke.rs.
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
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/cline/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/cline/src/cmd_*.rs | wc -l'
    expect: ">= 3"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/cline/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/cline/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/cline/src/**"
    - "plugins/cline/tests/**"
    - "plugins/cline/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/cline/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59–63 and 65–69.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs changes; remove tests. No on‑disk format change or user-visible CLI changes required.
