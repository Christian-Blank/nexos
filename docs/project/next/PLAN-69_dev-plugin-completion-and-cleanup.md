---
plan_id: "69"
slug: "dev-plugin-completion-and-cleanup"
status: "proposed"
owner: "@cli/dri"
labels: ["plugins","blueprint-v1","cleanup","derive","tests","docs","agents"]
created_at: "2025-09-08T00:00:00Z"
source: "next"
risk: "low"
blast_radius: "internal-only"
depends_on: ["58"]
---

# PLAN‑69 — Dev plugin completion and cleanup

Mission: Finish the dev plugin migration by replacing remaining stub handlers with real implementations, consolidating parsing into `cmd_*` modules, ensuring complete ToolSpecs coverage (where applicable), expanding tests, and refreshing docs. This solidifies the Blueprint v1 pattern in our “meta” plugin.

## Outcomes (acceptance criteria)

1. Replace stub handlers
   - For each of the following modules, implement real `run()` logic (no “stub” envelopes):
     - `cmd_doctor.rs`: keep both “ci” and non‑ci modes working; assert checks envelopes.
     - `cmd_repo.rs`: implement read‑only “stub” routes to proper repo checks or clearly mark as intentionally non‑agent; remove placeholder code.
     - `cmd_schemas.rs`: ensure `dev schemas export` writes files and emits ctx.ok with file list.
     - `cmd_plans.rs`: ensure all subcommands (`list,next,status,start,finalize,pause,resume,archive,dump,requeue,verify,index build`) are correctly wired and tested at least with smoke tests already present at bin level.
     - `cmd_plugins_check.rs`: return computed structure report (as today) and respect `--strict`.
     - `cmd_log.rs`: ensure `rotate` and `status` working; avoid TTY‑dependent branches in CI.
     - `cmd_docs.rs`: already complete; keep as is.
   - Move any remaining parsing logic from `lib.rs` into `cmd_*` modules if still present.

2. ToolSpecs coverage
   - Ensure ToolSpecs are provided for agent‑facing commands only:
     - Must have: `dev.plugins.check`, `dev.doctor`, `dev.telemetry.report`, `dev.plans.*` where present in catalog, `dev.repo.*` results if intended for agents.
     - `dev.docs.commands` and `dev.log.*` can remain non‑agent (toolspec=null) if intended as infra helpers.
   - Verify exported schemas match current envelopes.

3. Tests
   - Add or extend co‑located tests under `plugins/dev/tests/`:
     - smoke tests for `plugins check`, `doctor` (non‑CI mode), `telemetry report`, and one plan lifecycle command (e.g., `plans status`).
     - Keep bin‑level E2E tests as is.

4. Docs and catalog
   - `nexoscli dev docs commands --apply-readmes` updates `plugins/dev/README.md` AUTO (id=commands).
   - Catalog parity (`--check-parity`) stays green.

5. Lists and gates
   - Ensure all `plugins/dev/src/cmd_*.rs` are listed in `.nexoscli/migrated/args.txt`.
   - `nexoscli dev repo args-enforce --migrated ...` passes with no violations for dev plugin.
   - `nexoscli dev plugins check --strict` passes.

## Non‑goals

- Changing user‑visible semantics of dev commands beyond correcting stubs and consolidating parsing.
- Adding new commands or ToolSpecs beyond alignment.

## Work plan

1) Replace stub `run()` implementations with real logic across remaining dev modules.
2) Confirm `lib.rs` is a thin router; any lingering parsing moved to modules.
3) Review ToolSpecs completeness and correctness; align with catalog.
4) Add/extend tests in `plugins/dev/tests/` for key commands (fast, isolated).
5) Regenerate docs and update README.
6) Update lists and verify gates locally:
   ```
   nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
   nexoscli dev plugins check --strict
   nexoscli dev repo args-enforce --migrated .nexoscli/migrated/args.txt --allow-list .nexoscli/allow/argmatches.txt
   nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity
   cargo test --workspace
   ```

## Must‑pass invariants (executable)

must_pass:
  - name: "no-stubs-remain"
    cmd: 'rg -n --glob "plugins/dev/src/**/*.rs" "ctx\\.ok\\(\\"dev\\",\\"stub\\"" | wc -l'
    expect: "== 0"

  - name: "no-argmatches-in-cmds"
    cmd: 'rg -n "get_one::<|get_flag\\(" plugins/dev/src/cmd_*.rs | wc -l'
    expect: "== 0"

  - name: "dual-derive-present"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/dev/src/cmd_*.rs | wc -l'
    expect: ">= 3"

  - name: "toolspec-parity"
    cmd: 'nexoscli dev docs commands --out-json docs/catalog/commands.json --check-parity'
    expect_exit: 0

  - name: "readme-auto-commands"
    cmd: |
      nexoscli dev docs commands --apply-readmes
      rg -n --glob "plugins/dev/README.md" "<!-- BEGIN: AUTO (id=commands) -->" | wc -l
    expect: "== 1"

  - name: "tests-present"
    cmd: 'rg -n --glob "plugins/dev/tests/*.rs" . | wc -l'
    expect: ">= 2"

## Change budget

change_budget:
  allowed_files:
    - "plugins/dev/src/**"
    - "plugins/dev/tests/**"
    - "plugins/dev/README.md"
    - "docs/CLI_REFERENCE.md"
    - "docs/catalog/**"
    - ".nexoscli/**"
  forbidden_patterns:
    - "todo!\\("
    - "unimplemented!\\("
  size_limits:
    - path_glob: "plugins/dev/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000

## Parallelization and dependencies

- depends_on: ["58"] (gates in place).
- Can run in parallel with 59–68; it does not require other plugins to be migrated.
- Contributes to PLAN‑70 repository hardening.

## Rollback

- Revert changed modules and tests in dev plugin. No on‑disk format change or user‑visible CLI changes required.
