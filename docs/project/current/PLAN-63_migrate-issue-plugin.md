---
plan_id: '63'
slug: migrate-issue-plugin
status: in-progress
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
task_id: TASK-20250909-plan-63-migrate-issue-plugin-to-plugin-blueprint-v1
started_at: 2025-09-09T13:46:51.723041Z
---
# PLAN‑63 — Migrate issue plugin to Plugin Blueprint v1

Mission: Convert the issue plugin to modular `cmd_*` structure with dual‑derive Args/Result, ToolSpecs alignment, tests, and README AUTO updates. Ensure commands remain agent‑grade where appropriate.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - `plugins/issue/src/cmd_list.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct IssueListArgs { #[arg(long, default_value="all")] pub state: Option<String> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct IssueListResult { items: Vec<IssueRow> } /* or mirror current schema */`
     - `subcommand()` via `IssueListArgs::augment_args(Command::new("list").about("List issues"))`
     - `run()` uses `IssueListArgs::from_arg_matches(m)`
   - `plugins/issue/src/cmd_show.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct IssueShowArgs { #[arg(required=true)] pub id: String }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct IssueShowResult { /* fields per current result schema */ }`
     - `subcommand()` via `IssueShowArgs::augment_args(Command::new("show").about("Show an issue"))`
     - `run()` parses via `IssueShowArgs::from_arg_matches(m)`
   - `plugins/issue/src/cmd_new.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct IssueNewArgs { #[arg(long)] pub title: String, #[arg(long)] pub body: Option<String>, #[arg(long)] pub labels: Option<Vec<String>> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct IssueNewResult { /* id, created_at, state, title, labels, etc. */ }`
     - `subcommand()` via `IssueNewArgs::augment_args(Command::new("new").about("Create a new issue"))`
     - `run()` parses via `IssueNewArgs::from_arg_matches(m)`
   - `plugins/issue/src/cmd_close.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct IssueCloseArgs { #[arg(long)] pub id: String, #[arg(long)] pub reason: Option<String> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct IssueCloseResult { id: String, state: String }`
     - `subcommand()` via `IssueCloseArgs::augment_args(Command::new("close").about("Close an issue"))`
     - `run()` parses via `IssueCloseArgs::from_arg_matches(m)`
   - `plugins/issue/src/cmd_import.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct IssueImportArgs { #[arg(long)] pub dry_run: bool, #[arg(long)] pub fixture: Option<String>, #[arg(long)] pub from: String, #[arg(long)] pub labels: Option<String>, #[arg(long, default_value_t=100)] pub limit: u32, #[arg(long)] pub path: Option<String>, #[arg(long)] pub repo: Option<String>, #[arg(long)] pub since: Option<String>, #[arg(long, default_value="open")] pub state: Option<String> }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct IssueImportResult { /* counters + rows per current schema */ }`
     - `subcommand()` via `IssueImportArgs::augment_args(Command::new("import").about("Import issues from external sources (read-only)"))`
     - `run()` parses via `IssueImportArgs::from_arg_matches(m)`
   - Update `plugins/issue/src/lib.rs` to be a thin router importing the modules, routing to their `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` usage in new `cmd_*` modules.
   - Flags synthesized via `Args::augment_args`; typed parsing via `FromArgMatches`.

3. ToolSpecs:
   - Ensure `tool_specs()` includes:
     - `issue.list`, `issue.show`, `issue.new`, `issue.close` with correct args/result schemas.
   - Keep `issue.import` as toolspec=null (catalog currently shows null) unless we intentionally expose it; document decision.

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/issue/README.md` AUTO (id=commands).
   - Parity (`--check-parity`) remains green; ToolSpec names unchanged.

5. Tests:
   - Add `plugins/issue/tests/smoke.rs` with minimal tests invoking:
     - `issue list --output json` and asserting an envelope structure.
     - `issue show --id X --output json` (use a fixture or mock path if appropriate), asserting envelope structure.

6. Lists:
   - Append new module paths to `.nexoscli/migrated/args.txt`.
   - Ensure none of the files are in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changing external integrations’ semantics; focus on structure and derives.
- Adding new commands.

## Work plan

1) Implement `cmd_list.rs`, `cmd_show.rs`, `cmd_new.rs`, `cmd_close.rs`, `cmd_import.rs`.
2) Update `src/lib.rs` router and `tool_specs()`.
3) Add tests/smoke.rs for list/show (and optionally new/close dry-run if environment allows).
4) Regenerate docs and README.
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
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/issue/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/issue/src/cmd_*.rs | wc -l'
    expect: ">= 4"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/issue/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/issue/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/issue/src/**"
    - "plugins/issue/tests/**"
    - "plugins/issue/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/issue/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59–62 and 64–69.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs router changes; remove tests. No on‑disk format change or user-visible CLI changes required.
