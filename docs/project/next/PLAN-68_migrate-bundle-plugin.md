---
plan_id: "68"
slug: "migrate-bundle-plugin"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","derive","tests","docs","agents"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "low"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑68 — Migrate bundle plugin to Plugin Blueprint v1

Mission: Convert the bundle plugin to modular `cmd_*` structure with dual‑derive Args/Result, ToolSpecs alignment, tests, and README AUTO updates. Keep agent‑grade ToolSpecs for build/verify.

## Outcomes (acceptance criteria)

1. Structure (split into modules):
   - `plugins/bundle/src/cmd_build.rs` — `bundle build`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct BundleBuildArgs { #[arg(long)] pub include: Option<String>, #[arg(long)] pub out: Option<String>, #[arg(required=true)] pub node_id: String }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct BundleBuildResult { path: String, inputs: Vec<String>, checksums: std::collections::BTreeMap<String,String> }`
     - `subcommand()` via `BundleBuildArgs::augment_args(Command::new("build").about("Build a bundle (.tgz) with manifest and inputs"))`
     - `run()` parses via `BundleBuildArgs::from_arg_matches(m)`; emits `ctx.ok("bundle","build", ...)`
   - `plugins/bundle/src/cmd_verify.rs` — `bundle verify`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, clap::Args, Debug, Clone)] pub struct BundleVerifyArgs { #[arg(long)] pub against: Option<String>, #[arg(required=true)] pub path: String }`
     - `#[derive(Serialize, Deserialize, schemars::JsonSchema, Debug, Clone)] pub struct BundleVerifyResult { archive_ok: bool, manifest_ok: bool, against_ok: Option<bool>, errors: Vec<String> }`
     - `subcommand()` via `BundleVerifyArgs::augment_args(Command::new("verify").about("Verify a bundle"))`
     - `run()` parses via `BundleVerifyArgs::from_arg_matches(m)`
   - Update `plugins/bundle/src/lib.rs` to be a thin router importing modules and routing to `run()`.

2. Parsing and derives:
   - No imperative `ArgMatches` in new `cmd_*` modules.
   - Typed parsing via `FromArgMatches`; flags via `Args::augment_args`.

3. ToolSpecs:
   - Ensure `tool_specs()` includes:
     - `bundle.build` and `bundle.verify` with matching args/result schemas (as already reflected in catalog).

4. Docs and catalog:
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/bundle/README.md` AUTO (id=commands).
   - Catalog parity (`--check-parity`) remains green.

5. Tests:
   - Add `plugins/bundle/tests/smoke.rs` with minimal tests invoking:
     - `bundle build --node-id X --output json` (dry path or fixture) asserting envelope structure.
     - `bundle verify --path ./testdata/sample.tgz --output json` (skip in CI if fixture absent) asserting envelope structure.

6. Lists:
   - Append new module paths to `.nexoscli/migrated/args.txt`.
   - Ensure none are present in `.nexoscli/allow/argmatches.txt`.

## Non‑goals

- Changing bundle format or verification semantics; structural migration only.

## Work plan

1) Implement `cmd_build.rs` and `cmd_verify.rs` with Args/Result, `subcommand()`, `run()`.
2) Update `src/lib.rs` router and `tool_specs()`.
3) Add tests/smoke.rs for build/verify happy paths (fixture‑gated if necessary).
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
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/bundle/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/bundle/src/cmd_*.rs | wc -l'
    expect: ">= 2"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/bundle/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'test -f plugins/bundle/tests/smoke.rs'
    expect_exit: 0

## Change budget

change_budget:
  allowed_files:
    - "plugins/bundle/src/**"
    - "plugins/bundle/tests/**"
    - "plugins/bundle/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/bundle/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with Plans 59–67 and 69.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert module split and lib.rs updates; remove tests. No on‑disk format change; user-visible CLI unchanged.
