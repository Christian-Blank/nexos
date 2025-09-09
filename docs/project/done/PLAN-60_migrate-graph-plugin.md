---
plan_id: '60'
slug: migrate-graph-plugin
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
completed_at: 2025-09-09T03:31:45.059486Z
implemented_in: fed6a29
---
# PLAN‑60 — Migrate graph plugin to Plugin Blueprint v1

Mission: Convert the graph plugin to the modular `cmd_*` structure with dual‑derive Args, ToolSpecs, tests, and README AUTO updates. This ensures graph commands are agent‑grade and schema‑documented.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - `plugins/graph/src/cmd_check.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct GraphCheckArgs { #[arg(long, default_value="docs/exec_graph.yaml")] pub file: String }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct CheckSummary { acyclic: bool, unique_ids: bool, node_count: u32, errors: Vec<String> }`
     - `subcommand()` built via `GraphCheckArgs::augment_args(Command::new("check").about("Validate docs/exec_graph.yaml (acyclic, unique ids)"))`
     - `run()` parses via `GraphCheckArgs::from_arg_matches(m)` and emits `ctx.ok("graph","check", ...)`.
   - `plugins/graph/src/cmd_impact.rs`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct GraphImpactArgs { #[arg(long, default_value="docs/exec_graph.yaml")] pub file: String, #[arg(required=true)] pub id: String }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct GraphImpactResult { id: String, impacted: Vec<String> }`
     - `subcommand()` via `GraphImpactArgs::augment_args(Command::new("impact").about("List nodes impacted (downstream) by the given id"))`
     - `run()` parses via `GraphImpactArgs::from_arg_matches(m)`.
   - Update `plugins/graph/src/lib.rs` to be a thin router importing `cmd_check` and `cmd_impact`, routing to their `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` parsing in the new `cmd_*` modules.
   - `FromArgMatches` used; `Args::augment_args` used to build flags.

3. ToolSpecs:
   - Ensure `tool_specs()` exposes:
     - `graph.check` with `args_schema` from `GraphCheckArgs` and `result_schema` from `CheckSummary`.
     - `graph.impact` with `args_schema` from `GraphImpactArgs` and `result_schema` from `GraphImpactResult`.

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/graph/README.md` AUTO (id=commands).
   - Catalog parity (`--check-parity`) remains green; toolspec names unchanged.

5. Tests:
   - Add `plugins/graph/tests/smoke.rs` with minimal integration tests invoking:
     - `graph check --output json` and asserting envelope shape and presence of fields.
     - `graph impact --id X --output json` asserting envelope fields (OK to use a fixture or stubbed behavior as current implementation allows).

6. Lists:
   - Append new module paths to `.nexoscli/migrated/args.txt`.
   - Ensure these files are not present in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Altering graph semantics beyond modularization and derives.
- Redesigning exec_graph.yaml format (use current contract).

## Work plan

1) Implement `cmd_check.rs` and `cmd_impact.rs` with Args/Result, `subcommand()`, `run()`.
2) Update `src/lib.rs` to import modules, register subcommands, and expose ToolSpecs.
3) Add tests/smoke.rs for check and impact happy paths (assert `ctx.ok` JSON envelope).
4) Export docs and update plugin README (AUTO block).
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
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/graph/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/graph/src/cmd_*.rs | wc -l'
    expect: ">= 2"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/graph/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/graph/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/graph/src/**"
    - "plugins/graph/tests/**"
    - "plugins/graph/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/graph/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with other plugin migrations (59, 61–69).
- Does not block others directly; contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs routing changes; remove tests. No on‑disk format change or user-visible CLI changes required.
