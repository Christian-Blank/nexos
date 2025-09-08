---
plan_id: '55'
slug: command-catalog-and-schemas
status: done
owner: '@cli/dri'
labels:
- docs
- catalog
- json-schema
- llm-friendliness
- derive-enforcement
- tests
- ci-gates
created_at: 2025-09-07T00:00:00Z
source: next
risk: low
blast_radius: internal-only
depends_on:
- '54'
task_id: TASK-20250908-plan-55-agent-grade-command-catalog-schema-gates-commands-json-toolspec-parity
started_at: 2025-09-08T01:15:32.683449Z
completed_at: 2025-09-08T01:15:50.576574Z
implemented_in: a6df198
---
# PLAN‑55 — Agent‑grade command catalog & schema gates (commands.json + ToolSpec parity)

> **Mission:** Export a machine‑readable command catalog that LLM agents can consume directly, and enforce **Args dual‑derive** + ToolSpec/schema parity across the repo.

## Outcomes (acceptance criteria)

1. **Machine catalog:** `nexoscli dev docs commands --out-json docs/catalog/commands.json` writes a stable JSON with:
   - For each command `path`: human metadata (`about`, `usage`) and **args** with normalized properties: `name`, `long`, `short`, `kind`, `required`, `default`, `possible_values`, `help`.
   - Cross‑links to **ToolSpec** (when available): `tool.name`, `args_schema_ref`, `result_schema_ref`.
2. **Catalog schema:** `docs/catalog/schema/commands_catalog.schema.json` (JSON Schema) describing the catalog shape. A CI step validates generated `commands.json` against it.
3. **Dual‑derive enforcement:** For every migrated command family, the `Args` struct derives:

#[derive(clap::Args, schemars::JsonSchema, Serialize, Deserialize, Debug, Clone)]

and is **parsed via** `T::from_arg_matches(m)`. A repo check fails if ArgMatches parsing is used for those commands.
4. **Parity checks:** CI ensures:
- Every ToolSpec’s `args_schema`/`result_schema` is present.
- Each command with a ToolSpec appears in the catalog and vice‑versa (no orphan specs or commands).
5. **Docs drift gated:** Combined with PLAN‑54, CI fails if either `docs/CLI_REFERENCE.md` or `docs/catalog/commands.json` changes after regeneration.

## Non‑goals

- Human‑rendered docs format changes (already handled in PLAN‑54).
- Introducing a new DSL for defining commands.

---

## Design

### A. JSON layout (stable keys)

`docs/catalog/commands.json`:

```json
{
"schema": "commands_catalog.v1",
"generated_at": "2025-09-07T00:00:00Z",
"root": "nexoscli",
"version": "<bin version>",
"commands": [
 {
   "path": "dev init",
   "about": "Initialize .nexoscli/ (Plan/Apply, idempotent)",
   "usage": "nexoscli dev init [--dry-run] [--yes] [--with-config]",
   "plugin": "dev",
   "args": [
     {"name":"dry-run","long":"dry-run","short":null,"kind":"flag","required":false,"default":null,"possible_values":null,"help":"Show plan only"}
   ],
   "toolspec": {
     "name": "dev.init",
     "args_schema_ref": "schemas/dev.init.args.json",
     "result_schema_ref": "schemas/dev.init.result.json"
   }
 }
],
"schemas": {
 "dev.init.args.json": { /* schemars output (inlined or $ref) */ },
 "dev.init.result.json": { /* … */ }
}
}

Notes:
	•	Keep commands flat with unique path keys; derive ancestry by splitting on spaces.
	•	schemas can be inlined (as above) or paths to files under docs/catalog/schemas/ (see Interfaces below). Choose one and keep it stable.

B. Export flow
	•	Reuse PLAN‑54’s __introspect clap-json output for structural metadata.
	•	Query each loaded plugin’s tool_specs() to map commands → tools (pattern: <plugin>.<cmd> or explicit mapping table if needed).
	•	When a ToolSpec exists, serialize its args_schema and result_schema into schemas (or write them as files and reference them).

C. Derive enforcement & ArgMatches ban (for migrated commands)
	•	Add a repo check: error if any migrated command family uses m.get_one, m.get_flag, value_of, etc.
	•	The check should be scoped by a allow‑list for commands not yet migrated (keeps adoption incremental).

D. Validation & drift
	•	Validate commands.json against docs/catalog/schema/commands_catalog.schema.json in CI.
	•	After regeneration, commit or fail on diff (same pattern as PLAN‑54).

⸻

Agent Runbook (deterministic)
	1.	Catalog export
	•	Implement nexoscli dev docs commands --out-json docs/catalog/commands.json.
	•	Verify:

nexoscli dev docs commands --out-json docs/catalog/commands.json --output json
jq -e '.schema=="commands_catalog.v1"' docs/catalog/commands.json
jq -e '.commands|length > 0' docs/catalog/commands.json


	2.	Schema generation
	•	Write docs/catalog/schema/commands_catalog.schema.json (hand‑authored) and add a CI validation step:

jq -e '.' docs/catalog/commands.json > /dev/null
# If you have a JSON Schema validator in the repo, run it here


	3.	ToolSpec parity
	•	Ensure every ToolSpec (tool_specs()) is referenced:

nexoscli tools export --output json | jq -r '.data.tools[].name' > /tmp/tools.txt
jq -r '.commands[].toolspec.name // empty' docs/catalog/commands.json > /tmp/catalog-tools.txt
comm -3 <(sort /tmp/tools.txt) <(sort /tmp/catalog-tools.txt) | wc -l
# Expect 0 (no mismatch)


	4.	Dual‑derive & no ArgMatches
	•	Enforce derive for migrated families:

rg -n '#$begin:math:display$derive\\([^]]*clap::Args[^]]*\\)$end:math:display$' plugins/**/src/cmd_*.rs | wc -l
# Expect >= N (migrated commands)
rg -n --glob "plugins/**/src/cmd_*.rs" 'get_one::<|get_flag\(' | wc -l
# Expect 0 for migrated commands (ArgMatches banned)


	•	If some families aren’t migrated yet, maintain an allow‑list file (e.g., .nexoscli/allow/argmatches.txt) and check absence outside allow‑list.

	5.	Docs drift
	•	Run both exporters and ensure no diffs:

nexoscli dev docs commands --out-md docs/CLI_REFERENCE.md --out-json docs/catalog/commands.json --apply-readmes
git add -A && git diff --cached --quiet



⸻

Must‑pass invariants (executable)

must_pass:
  - name: "catalog-exists"
    cmd: 'test -s docs/catalog/commands.json'
    expect_exit: 0

  - name: "catalog-schema-tag"
    cmd: 'jq -e ".schema==\"commands_catalog.v1\"" docs/catalog/commands.json'
    expect_exit: 0

  - name: "toolspec-parity"
    cmd: |
      nexoscli tools export --output json | jq -r '.data.tools[].name' | sort > /tmp/tools.txt
      jq -r '.commands[].toolspec.name // empty' docs/catalog/commands.json | sort > /tmp/catalog-tools.txt
      comm -3 /tmp/tools.txt /tmp/catalog-tools.txt | wc -l
    expect: "== 0"

  - name: "derive-enforced"
    cmd: 'rg -n "#\\[derive\\([^]]*clap::Args" plugins/**/src/cmd_*.rs | wc -l'
    expect: ">= 1"

  - name: "no-argmatches-migrated"
    cmd: 'rg -n --glob "plugins/**/src/cmd_*.rs" "get_one::<|get_flag\\(" | wc -l'
    expect: "== 0"   # or use allow-list until full migration

  - name: "catalog-no-drift"
    cmd: |
      nexoscli dev docs commands --out-json docs/catalog/commands.json
      git add -A && git diff --cached --quiet
    expect_exit: 0


⸻

Interfaces & files produced
	•	docs/catalog/commands.json (machine)
	•	docs/catalog/schema/commands_catalog.schema.json (validator)
	•	Optional (if externalized): docs/catalog/schemas/*.json (per ToolSpec arg/result)

⸻

Change budget

change_budget:
  allowed_files:
    - "nexoscli-bin/src/**"                # for introspection tweaks if needed
    - "plugins/dev/src/cmd_docs.rs"        # JSON path support
    - "plugins/**/src/cmd_*.rs"            # to add dual-derive
    - "plugins/**/src/lib.rs"
    - "docs/catalog/**"
    - ".github/workflows/**"               # or CI equivalent
  forbidden_patterns:
    - 'ctx\\.ok\\("dev","stub"'
    - 'todo!\\('
    - 'unimplemented!\\('
    - 'ArgMatches\\b.*get_one'             # outside allow-list
    - 'ArgMatches\\b.*get_flag'            # outside allow-list
  file_size_limits:
    - path_glob: "plugins/*/src/*.rs"
      warn_loc: 1000
      fail_loc: 2000


⸻

Tests
	•	Snapshot test for docs/catalog/commands.json (keys sorted; tolerate value changes that are time‑based).
	•	Unit test: toolspec-parity (load ToolSpecs from a test plugin and ensure presence in catalog).
	•	Fixture test: ensure exporter merges possible_values and defaults correctly.

⸻

Rollback
	•	Delete docs/catalog/** and remove JSON export branch from dev docs commands. No user‑visible CLI changes require rollback.

---

What this gives you immediately
	•	A single, live source for human docs (PLAN‑54) and a machine‑grade catalog (PLAN‑55).
	•	Strict, executable invariants that Cline can run to check progress.
	•	Dual‑derive enforcement to keep arguments in one place and stop drift.
	•	Parities between code, docs, and ToolSpecs, so agents/humans share the same truth.