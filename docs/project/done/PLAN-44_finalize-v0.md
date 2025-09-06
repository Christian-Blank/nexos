---
status: done
plan_id: '44'
slug: finalize-v0
task_id: TASK-20250906-snapshots-are-automatic-on-extract-no-new-flags
started_at: 2025-09-06T08:08:59.380258Z
source: next
completed_at: 2025-09-06T08:14:10.943004Z
implemented_in: 898105f
---
PR‑v0 — Finalization Plan for Working v0

(combines #40, #41, #42, #43, #44 into one cohesive change set)

Status: proposal (ready to implement)
Scope labels: replay, dev, cline, observability, docs
Depends on: ✅ #34–#39 complete (schemas, fileops, export CSV, annotations add/list, cline correlation, agent‑grade capture)

⸻

0) Objectives (what “v0” means)
	•	A TracePack (“bag”) extracted from any recent run is self‑describing, valid, and analyzable:
	•	Env & repo snapshot recorded in manifest. (#40)
	•	Verify++ guards: schemas, counts, patch dry‑run, cline coverage warning. (#41)
	•	Analyst affordances: bag/README dataset card; analyst quickstart; labels injected into manifest.labels and events.ndjson rows for easy A/B filtering. (#42, #43)
	•	Doctor v0 profile: one flag provides sensible CI gates and visibility. (#41)
	•	One end‑to‑end acceptance test in CI proves extract → verify → annotate → export → doctor works on a synthetic repo. (#44)

⸻

1) User‑visible outcomes

CLI (new/extended)

# Snapshots are automatic on extract (no new flags)

# Verify++ (schema, counts, patches, cline coverage)
nexoscli replay verify --bag ./bag

# Add/merge experiment labels (bag + every events row)
nexoscli replay annotate labels --bag ./bag \
  --labels exp_id=EXP-001 treatment=A env=dev \
  [--from-file labels.json]

# Rebuild dataset card
nexoscli replay verify --bag ./bag --fix-readme

# Doctor CI with sensible defaults
nexoscli dev doctor --ci --profile v0

Bag layout (additions highlighted)

bag/
  events.ndjson
  fileops.ndjson
  manifest.json        # now includes: tools, env, git, labels, counts.*
  README.md            # ← dataset card (#43)
  patches/
  status/
  cline/...


⸻

2) Design summary (by plan)

#40 — Env & repo snapshot (additive)
	•	Capture tools (nexoscli, rustc, cargo), env (os, arch), and git (branch, head, dirty) and write to manifest.json on extract.

#41 — Verify++ & Doctor v0
	•	replay verify:
	•	JSON Schema validation for events.ndjson (nexos/events@0.3) and fileops.ndjson (nexos/fileops@0.1).
	•	Count checks vs manifest.counts.
	•	Patch dry‑run (git apply --check) if patches/*.patch present.
	•	Cline coverage warning if we see term.run events but no cline artifacts.
	•	dev doctor –ci –profile v0 preselects sensible checks (graph, plugins, config, events, cline, rules, tools) and marks a minimal set as required (e.g., graph_check, plugins_check, event_log_writable). Cline coverage is informational.

#42 — Post‑hoc labels (analyst‑ready)
	•	replay annotate **labels** upserts key/value pairs into manifest.labels and injects them into each events.ndjson row under attrs.labels (merge, don’t overwrite).
	•	Note on CLI shape: This is namespaced under annotate labels to coexist with the already‑shipped annotate add|list from #37.

#43 — Dataset card & Analyst quickstart
	•	Always write bag/README.md on extract (re‑writable via verify --fix-readme): contents & schema, counts, labels, tool/env/git snapshots, and copy‑paste snippets (Pandas, DuckDB).
	•	Add docs/analysis/QUICKSTART.md.

#44 — v0 E2E Acceptance
	•	Single test proving end‑to‑end (“hello‑world repo” → term.run → extract → verify → annotate labels → export CSV (already present) → doctor v0).

⸻

3) Patches (where and what)

The snippets below are integrations on top of your current tree. They reuse patterns already present in plugins/*.

3.1 plugins/replay/src/lib.rs — #40 + #41 + #42 + #43 hooks
	•	Add helpers (or util.rs) for snapshots:
	•	collect_tools_snapshot(), collect_env_snapshot(), collect_git_snapshot(repo_root)
	•	On extract, after manifest skeleton:
	•	manifest["tools"] = collect_tools_snapshot();
	•	manifest["env"]   = collect_env_snapshot();
	•	manifest["git"]   = collect_git_snapshot(&repo_root);
	•	write_dataset_card(&bag_dir, &manifest).ok();
	•	Extend replay verify to:
	•	Validate NDJSON with bundled schemas (from #34) and report {ok, lines, valid, invalid, errors[..]}.
	•	Compare counts with manifest.counts.
	•	git apply --check all patches/*.patch (aggregate boolean).
	•	Cline coverage guard (warn only).
	•	--fix-readme: rebuild dataset card from manifest/files present.
	•	Add subcommand: annotate labels (adjacent to annotate add|list from #37):
	•	--bag <DIR> [--labels K=V]* [--from-file labels.json]
	•	Upsert manifest.labels, inject into each events.ndjson line under attrs.labels (merge).
	•	Envelope includes {total_rows, skipped_lines} (invalid JSON rows skipped, not fatal).
	•	Dataset card helper write_dataset_card(bag, manifest) (stable JSON blocks, Pandas/DuckDB snippets).
	•	(Optional) expose tool specs for agents:
	•	replay.validate, replay.annotate.labels, replay.schema.* (already from #34).

3.2 plugins/dev/src/lib.rs — #41
	•	Doctor v0 profile:
	•	--profile v0 presets the checks set and inserts a small required set.
	•	Cline coverage line displays coverage=% over N runs (info).
	•	(Optional debug) dev env print returns the same snapshots we embed into manifest.

3.3 docs/** — #43
	•	New: docs/analysis/QUICKSTART.md.
	•	Update: docs/TRACEPACK.md to mention:
	•	manifest.tools|env|git
	•	Dataset card (bag/README.md)
	•	manifest.labels and the attrs.labels injection.
	•	Update: docs/PLAYBOOK.md to include the Doctor v0 profile note.

3.4 Tests — #41 + #42 + #43 + #44
	•	nexoscli-bin/tests/v0_e2e.rs (#44):
Arrange a tiny repo, run a couple of term run commands, synthesize minimal cline messages file, extract bag, verify, annotate labels, export, doctor –profile v0.
	•	nexoscli-bin/tests/v0_labels_card.rs (#42 + #43):
Extract → annotate labels → assert manifest.labels + attrs.labels and bag/README.md sections.
	•	Schema validate test replay_schema_validate.rs already exists from #34; keep it green with new validators.
	•	FileOps test (from #35) remains green; verify counts still match.

⸻

4) CLI contract (concise)

# Validate a bag (schema + counts + patches + cline guard)
nexoscli replay verify --bag <DIR> [--fix-readme]

# Add labels (bag + every events row)
nexoscli replay annotate labels --bag <DIR> \
  --labels exp_id=EXP-001 --labels treatment=A [--from-file labels.json]

# Doctor gates and coverage
nexoscli dev doctor --ci --profile v0


⸻

5) Data model (additions)
	•	manifest.json (new top‑level keys):
	•	tools: { nexoscli?, rustc?, cargo? }
	•	env: { os, arch }
	•	git: { branch?, head?, dirty: bool }
	•	labels: { <K>: <V>, ... }
	•	events.ndjson (unchanged schema; usage change):
	•	Each row’s attrs.labels gets merged K/V (post‑hoc by annotate labels).

⸻

6) Acceptance criteria (Definition of Done)
	•	Extract writes snapshots (tools, env, git) into manifest; dataset card exists.
	•	Verify reports:
	•	events.ndjson and fileops.ndjson schema‑valid (ok=true),
	•	counts match manifest,
	•	patches check is true when patches exist,
	•	cline_coverage warning when term.run present but cline artifacts absent.
	•	Annotate labels:
	•	manifest.labels contains the upserts,
	•	every valid events row contains attrs.labels with merged keys,
	•	reports {total_rows >= 1, skipped_lines == 0} for normal bags.
	•	Doctor with --profile v0 returns success (required gates green) and a coverage line for cline (info).
	•	E2E test passes in CI (single file).

⸻

7) Rollout & compatibility
	•	Entirely additive; no breaking changes to schemas.
	•	Bags created before this PR can be annotated and given a dataset card via verify --fix-readme.
	•	Doctor profile is opt‑in (--profile v0), preserving current behavior when unused.

⸻

8) Risks & mitigations

Risk	Mitigation
Very large events.ndjson → rewriting for label injection is O(n).	v0 uses simple rewrite; metrics are modest. Optimize later (streaming rewrite to temp file).
git not installed or repo not initialized.	Snapshot is best‑effort (fields become null/false). Verify does not fail for missing git.
Malformed lines in events.ndjson.	Skip such lines during injection; report skipped_lines. Verify catches schema issues earlier.
CLI overlap with #37 annotate group.	Namespaced as replay annotate labels, coexisting with `add


⸻

9) Implementation checklist
	•	Replay: Snapshot helpers; manifest augmentation on extract (#40).
	•	Replay: Verify++ (schemas, counts, patches, cline guard; --fix-readme) (#41).
	•	Replay: annotate labels (manifest upsert + row injection) (#42).
	•	Replay: Dataset card writer and hook in extract/verify (#43).
	•	Dev: Doctor --profile v0 and cline coverage info (#41).
	•	Docs: TRACEPACK.md, PLAYBOOK.md, analysis/QUICKSTART.md (#43).
	•	Tests: v0_e2e.rs, v0_labels_card.rs (#44).
	•	Build & run workspace tests.

⸻

10) Suggested commits (atomic & reviewable)
	1.	feat(replay): env+repo snapshot in manifest; dataset card writer (Plan #40/#43)
	2.	feat(replay): verify++ (schema, counts, patches, cline guard) and --fix-readme (Plan #41/#43)
	3.	feat(replay): annotate labels (manifest+events attrs.labels merge) (Plan #42)
	4.	feat(dev): doctor --profile v0; cline coverage info (Plan #41)
	5.	docs: analysis quickstart; TRACEPACK additions (Plan #43)
	6.	test: v0_e2e + v0_labels_card (Plan #44)

(Can squash if preferred; the above order keeps each PR reviewable.)

⸻

11) Quick smoke (manual)

cargo build --workspace

# Seed a run
nexoscli dev init --yes
nexoscli term run -- bash -lc 'echo hi >> demo.txt'

# Extract → validate → label → readme → doctor
RUN=$(tail -n 1 .nexoscli/log/events.jsonl | jq -r '.run_id')
nexoscli replay extract --run-id "$RUN" --out bag/demo
nexoscli replay verify --bag bag/demo
nexoscli replay annotate labels --bag bag/demo --labels exp_id=EXP-001 treatment=A
nexoscli replay verify --bag bag/demo --fix-readme
nexoscli dev doctor --ci --profile v0

Expected: verify ok; manifest.labels present; events.ndjson rows include attrs.labels; bag/README.md has counts/labels/tooling; doctor shows cline coverage (info).

⸻

Appendix — Key helper snippets (for reference)

These are the exact helpers referenced; integrate where you keep utilities.

	•	Snapshots: collect_tools_snapshot(), collect_env_snapshot(), collect_git_snapshot().
	•	Validator: reuse bundled schemas from #34; run per‑line NDJSON validation; cap errors (~10–20).
	•	Dataset card: deterministic sections with pretty JSON blocks.
	•	Label injection: safe merge into attrs.labels; skip invalid rows; report counts.

⸻

This plan keeps v0 tight and binary: extract creates self‑describing bags; verify makes correctness observable; annotate makes A/B work easy; the dataset card & quickstart lower analyst friction; doctor provides a single, practical CI gate.