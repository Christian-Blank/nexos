v0 End‑to‑End Test Plan

A comprehensive, real‑use‑case–driven plan that exercises everything we landed in #34–#44, with checklists and concrete steps.

What this validates
	•	TracePack bags are self‑describing, valid, and analyzable (schemas, counts, dataset card).
	•	Replay verify++ catches schema/count/patch issues and warns on Cline coverage gaps.
	•	Env & repo snapshots are present in the bag manifest.
	•	Labels can be applied post‑hoc to manifest and every events row.
	•	Cline correlation works (import → correlate → include in bag).
	•	Exports (CSV) and annotation joins work for analysts.
	•	Doctor –profile v0 provides a useful CI gate with sensible defaults.

⸻

0) Test Environments & Pre‑flight

Environments to run on
	•	Local macOS (arm64 or x86_64)
	•	Local Linux (x86_64)
	•	Dev Container (VS Code / Docker; with and without host Cline storage mounted)
	•	CI (your repo’s CI environment)

Matrix (check all that apply)
	•	[ ] macOS local
	•	[ ] Linux local
	•	[ ] Dev Container (with host Cline storage mounted)
	•	[ ] Dev Container (no Cline storage mounted)
	•	[ ] CI pipeline

Prerequisites
	•	git installed and on PATH.
	•	Build passes: cargo build --workspace and cargo test --workspace.
	•	Repo initialized: nexoscli dev init --yes in a fresh test directory.

Conventions
	•	Use a fresh temp directory per scenario unless noted.
	•	All commands run inside the test repo root unless specified.
	•	Record the run_id returned by term run envelopes (or tail events.jsonl).

⸻

1) User Stories (End‑to‑End)

US‑A (Solo Dev w/ Agent)

“As a developer working with Cline, when I run commands and chat with the agent, I can extract a bag that includes my events, file changes, cline tasks/messages, and a dataset card. I can verify the bag, label it (A/B), export CSV, and doctor says my environment is healthy.”

US‑B (Human‑Only Dev)

“As a developer without any agent linked, I still get valid bags with env/repo snapshots, pass schema verification, and a dataset card. Verify warns me (soft) if agent coverage is zero.”

US‑C (Analyst)

“As an analyst receiving a bag, I can verify it, see a dataset card, add labels, and export CSV, including events_annotated.csv for filtering and basic metrics.”

US‑D (CI Gatekeeper)

“As a maintainer, I can run dev doctor --ci --profile v0 and get predictable pass/fail signals. If core checks fail, CI breaks; Cline coverage stays informational.”

⸻

2) Test Inventory (by Feature Group)

Each case includes steps, expected results, and a checklist. You can paste these into an issue or notebook and check them off.

Group A — Smoke & Happy Path E2E

A1 — Minimal run → extract → verify → label → export → doctor
	1.	cargo build --workspace
	2.	mkdir -p /tmp/nx-e2e-A1 && cd /tmp/nx-e2e-A1
	3.	nexoscli dev init --yes
	4.	nexoscli term run -- bash -lc 'echo hello > a.txt && git init && git config user.email t@e && git config user.name T && git add a.txt && git commit -m init'
	5.	Capture RUN=$(tail -n 1 .nexoscli/log/events.jsonl | jq -r '.run_id')
	6.	nexoscli replay extract --run-id "$RUN" --out bag/A1
	7.	nexoscli replay verify --bag bag/A1
	8.	nexoscli replay annotate labels --bag bag/A1 --labels exp_id=EXP-001 --labels treatment=A
	9.	nexoscli replay export --bag bag/A1
	10.	nexoscli dev doctor --ci --profile v0

Expect
	•	bag/A1/manifest.json has tools|env|git (some may be null/bool), and counts.
	•	bag/A1/README.md exists with counts, labels, tooling blocks.
	•	verify returns ok, with schema checks green, count checks true.
	•	events.ndjson rows contain attrs.labels for exp_id & treatment.
	•	bag/A1/exports/csv/ has events.csv, fileops.csv.
	•	doctor succeeds; cline coverage line shows info (likely 0% in this A1).

Checklist:
	•	[ ] Manifest has snapshots (tools, env, git)
	•	[ ] Dataset card exists (bag/A1/README.md)
	•	[ ] Verify ok (schemas & counts)
	•	[ ] Labels present in manifest.labels and events.attrs.labels
	•	[ ] Exports written (CSV)
	•	[ ] Doctor ok (required gates), cline coverage shows (info)

⸻

Group B — Extract & Manifest Snapshots (#40)

B1 — Snapshots with git repo
	•	Use A1 repo. Inspect manifest.json.
	•	Expect: git.branch set (e.g., master or main), git.head 40‑hex, git.dirty false.

B2 — Snapshots without git repo
	•	New dir: nexoscli dev init --yes then do not git init.
	•	Run term run (create a file) → extract → inspect manifest.
	•	Expect: git.branch/head may be null/absent; git.dirty false; verify still ok.

B3 — Dirty working tree
	•	In A1 repo: echo x >> a.txt (no commit) → term run 'true' → extract.
	•	Expect: git.dirty true.

B4 — Tools presence
	•	Expect: tools.nexoscli includes version string; rustc and cargo strings present when on PATH (can be null if absent).

Checklist:
	•	[ ] Git fields behave as expected across repo/no‑repo/dirty
	•	[ ] Tools recorded (best‑effort)
	•	[ ] Verify does not fail when git not present

⸻

Group C — Verify++ (#41)

C1 — Schema validation success
	•	Use bag from A1.
	•	nexoscli replay verify --bag bag/A1
	•	Expect: events_schema_ok=true, fileops_schema_ok=true; invalid=0.

C2 — Schema validation failure (events)
	•	Copy bag: cp -R bag/A1 bag/C2
	•	Corrupt one events line: append }{ at end of events.ndjson.
	•	nexoscli replay verify --bag bag/C2
	•	Expect: events_schema_ok=false, errors show line number; overall non‑zero exit.

C3 — Counts check
	•	Ensure verify output reports events_lines, fileops_lines and *_count_match=true.

C4 — Patch dry‑run success
	•	Expected for A1 (patches created by file changes).
	•	Expect: patches_check=true (or absent when no patches dir).

C5 — Patch dry‑run failure (negative)
	•	Copy bag A1 → bag/C5, then manually corrupt a patch file (edit a hunk header).
	•	nexoscli replay verify --bag bag/C5
	•	Expect: patches_check=false; verify non‑zero exit code.

C6 — Cline coverage warning
	•	With no Cline storage/import: A1 verify should include a warning if term.run present but cline artifacts absent.
	•	Expect: warnings includes cline_coverage_zero.

Checklist:
	•	[ ] Schemas fail with useful errors on malformed JSON
	•	[ ] Counts compared and match
	•	[ ] Patch dry‑run reports true/false accurately
	•	[ ] Cline coverage warning appears appropriately

⸻

Group D — Cline Correlation & Inclusion (#38 + #39 references)

You’ll run these twice: with and without Cline storage available.
For “with storage”, point the plugin at the globalStorage tasks dir or use the test harness pattern to create a fake storage tree and set the env var your cline locate honors.

D1 — No storage mounted
	•	nexoscli cline locate → expect no selected.
	•	nexoscli dev doctor --ci --profile v0 → cline check = info with guidance.

D2 — Import tasks (with storage)
	•	Ensure Cline storage path is available; nexoscli cline import --since 7d.
	•	Expect: .nexoscli/cline/tasks/*.json populated.

D3 — Correlate recent runs (dry‑run)
	•	Produce a term run.
	•	nexoscli cline correlate --since 24h --pad 5m --dry-run
	•	Expect: coverage summary (runs_total, runs_with_tasks, coverage).

D4 — Correlate a single run (persist)
	•	nexoscli cline correlate --run-id "$RUN" --pad 5m --write
	•	Expect: .nexoscli/cline/corr/<RUN>.json exists with tasks[].

D5 — Extract bag with cline included
	•	nexoscli replay extract --run-id "$RUN" --include cline --out bag/D5
	•	Expect: bag/D5/cline/tasks/*.json present; manifest.cline_tasks[] with bytes & sha256; counts.cline>=1.

D6 — Verify bag with cline
	•	nexoscli replay verify --bag bag/D5
	•	Expect: no cline coverage warning; everything ok.

D7 — Pad sensitivity
	•	Correlate again with --pad 0s vs --pad 30m, compare tasks count.
	•	Expect: larger pad yields ≥ tasks.

Checklist:
	•	[ ] locate/import/correlate behave as expected across storage/no‑storage
	•	[ ] correlation record persisted per run
	•	[ ] extract includes cline tasks & manifest lists them
	•	[ ] verify no cline warning when cline artifacts present

⸻

Group E — Labels (Post‑hoc) (#42)

E1 — Manifest upsert
	•	nexoscli replay annotate labels --bag bag/A1 --labels exp_id=EXP-002 --labels treatment=B
	•	Expect: manifest.labels contains exp_id=EXP-002, treatment=B (replacing/overriding previous).

E2 — Events row injection
	•	Open bag/A1/events.ndjson and grep for "attrs":{"labels":{"exp_id":"EXP-002","treatment":"B"}} (order may vary).
	•	Expect: each valid line contains merged labels.

E3 — Merge semantics
	•	Repeat labels with new key branch=main do not remove exp_id or treatment.
	•	Expect: attrs.labels has all keys (last value wins for duplicates).

E4 — From file
	•	Create labels.json = {"exp_id":"EXP-010","treatment":"A","experiment":"prompt-v1"}
	•	replay annotate labels --bag bag/A1 --from-file labels.json
	•	Expect: keys merged as above.

E5 — Idempotency
	•	Running the same label injection twice yields same results.

E6 — Malformed row tolerance
	•	If you intentionally corrupt one events row (see C2), injection skips it and reports skipped_lines=1; verify remains the place to catch schema faults.

Checklist:
	•	[ ] Manifest updated
	•	[ ] All valid events rows get labels
	•	[ ] Merge & idempotency confirmed
	•	[ ] Skips malformed rows and reports count

⸻

Group F — Dataset Card (#43)

F1 — Auto‑generated on extract
	•	Check bag/A1/README.md.
	•	Expect: sections: Contents & Schemas, Counts (JSON), Labels (JSON), Tooling (JSON), Load in Python, Load in DuckDB.

F2 — Rebuild via verify
	•	Delete bag/A1/README.md.
	•	nexoscli replay verify --bag bag/A1 --fix-readme
	•	Expect: README recreated; content matches manifest & files.

F3 — Contents reflect labels
	•	After E1/E4, re‑generate README (--fix-readme) and ensure Labels JSON shows all keys.

Checklist:
	•	[ ] README present on extract
	•	[ ] fix‑readme regenerates
	•	[ ] README content reflects manifest state

⸻

Group G — Exports & Annotation Joins (#36, #37)

G1 — Base CSVs
	•	nexoscli replay export --bag bag/A1
	•	Expect: exports/csv/events.csv, fileops.csv with headers:
	•	events: ts,run_id,plugin,command,exit_code,dur_ms,error_code,args_hash,content_hash,task_id,stage,session_id,attrs_cmd,attrs_cwd,attrs_json
	•	fileops: ts,run_id,op,path,from,to,bytes,digest_after,patch_sha256,patch_lines,source

G2 — Annotations add/list (from #37)
	•	replay annotate add --bag bag/A1 --target events --seq 0 --label slow --note "p90 high" --author tester --confidence 0.8
	•	replay annotate list --bag bag/A1 → count ≥ 1.

G3 — annotations.csv
	•	replay export --bag bag/A1 --what annotations
	•	Expect: exports/csv/annotations.csv with header including target_kind,target_seq.

G4 — Join annotations into events
	•	replay export --bag bag/A1 --what events --join annotations
	•	Expect: events_annotated.csv present; extra columns: seq,ann_labels,ann_note,ann_author,ann_source,ann_confidence,ann_count; row 0 contains your annotation content.

G5 — Join annotations into fileops
	•	replay export --bag bag/A1 --what fileops --join annotations
	•	Expect: fileops_annotated.csv present with similar ann_* fields.

Checklist:
	•	[ ] CSV schemas match
	•	[ ] annotations add/list work
	•	[ ] annotations.csv present
	•	[ ] events_annotated.csv and fileops_annotated.csv include ann_* columns and your note/labels

⸻

Group H — Doctor v0 Profile (#41)

H1 — Default success
	•	In A1 repo: nexoscli dev doctor --ci --profile v0
	•	Expect: exit 0; includes cline coverage info (not required).

H2 — Force required failure
	•	Temporarily make .nexoscli/log unwritable (chmod 0500 parent) or move away; run again.
	•	Expect: event_log_writable fails; non‑zero exit.

H3 — With Cline storage
	•	After D2: run doctor again.
	•	Expect: cline coverage greater than zero if you produced a run overlapping with tasks.

Checklist:
	•	[ ] Doctor succeeds under healthy env
	•	[ ] Doctor fails when required gates break
	•	[ ] Coverage message shows expected % and run counts

⸻

Group I — Idempotency & Determinism

I1 — Re‑extract same run to new path
	•	replay extract --run-id "$RUN" --out bag/I1
	•	Compare manifest.json → tool/env/git snapshots identical (except possibly created timestamps), counts equal.

I2 — Verify results stable
	•	Run replay verify twice → same result shape; no flakiness.

I3 — Label injection repeat
	•	Run annotate labels twice with same keys → no drift in events rows.

I4 — Dataset card stability
	•	Rebuild README twice → identical textual content (modulo generated time if present; our v0 card avoids volatile times in prose).

Checklist:
	•	[ ] Extract deterministic
	•	[ ] Verify stable
	•	[ ] Labels idempotent
	•	[ ] README stable

⸻

Group J — Performance (Sanity)

J1 — 10k event lines
	•	Generate 10k term run lines (loop), extract → verify.
	•	Expect: verify completes under acceptable time (sanity, not micro‑bench).

J2 — Label injection on large events
	•	Inject labels into the big bag.
	•	Expect: completes; reports processed row count accurately.

J3 — CSV export on large bag
	•	Exports finish and files openable by tools (head/tail OK).

Checklist:
	•	[ ] Verify scales to 10k lines
	•	[ ] Label injection scales
	•	[ ] Exports scale

⸻

Group K — Cross‑Platform Differences

K1 — Windows path quirks (if applicable to your support scope)
	•	Ensure any path serialization in manifest/cline lists uses forward slashes or consistent formatting acceptable to JSON tools.

K2 — macOS/Linux differences
	•	Snapshots produce expected os/arch values; git fields present/absent as per environment.

K3 — Dev Container
	•	Without host storage: cline coverage 0% (info).
	•	With host storage: coverage > 0 after runs.

Checklist:
	•	[ ] Paths & snapshots look sane on each OS
	•	[ ] Dev container cases covered

⸻

Group L — Negative & Failure Injection

L1 — Missing bag directory
	•	replay verify --bag no-such-dir → friendly error envelope; non‑zero exit.

L2 — Wrong --what for export
	•	replay export --bag bag/A1 --as parquet (not supported v0)
	•	Expect: friendly error: only CSV supported; non‑zero.

L3 — Corrupt annotations.ndjson
	•	Manually add invalid JSON line; export --what annotations → skips or fails with clear message (per implementation; v0 may error out explicitly).

L4 — Patches dir missing
	•	verify ok (patch dry‑run skipped or not applicable) — no false negatives.

L5 — Git absent
	•	Temporarily remove git from PATH; run A1; extract & verify still ok (snapshots best‑effort).

L6 — Malformed labels file
	•	annotate labels --from-file malformed.json → clear error envelope; no partial writes.

Checklist:
	•	[ ] Friendly errors & non‑zero exit codes
	•	[ ] No false negatives when artifacts legitimately absent
	•	[ ] Malformed inputs handled cleanly

⸻

Group M — Security & Privacy sanity

M1 — Manifest content
	•	Ensure no sensitive env variables are captured; only os, arch, git.branch/head/dirty, and tool versions.

M2 — Dataset card
	•	Verify README shows only bag‑internal facts: counts, labels (which you set), schemas, snapshots, no secrets.

M3 — Labels
	•	Confirm that labels are only what you add (no unintended keys appear).

Checklist:
	•	[ ] No secrets in manifest
	•	[ ] No secrets in README
	•	[ ] Labels under user control only

⸻

3) Evidence to Capture (per scenario)

For each group/case, attach:
	•	The command transcript (or the JSON envelopes where applicable).
	•	Key file snippets:
	•	bag/*/manifest.json (snipped)
	•	bag/*/README.md (first 50 lines)
	•	Any failing verify output (error list with line numbers)
	•	Exported CSV headers (first line) + wc -l row counts
	•	Exit codes (0/≠0).

⸻

4) Quick Checklists (Feature Coverage)

v0 Essentials
	•	Extract writes snapshots (tools/env/git)
	•	Verify validates schemas (events/fileops)
	•	Verify checks counts and patches
	•	Cline coverage warning (when applicable)
	•	Annotate labels → manifest & events rows
	•	Dataset card present & rebuildable
	•	Exports CSV (events/fileops/annotations)
	•	Joins: events_annotated.csv / fileops_annotated.csv
	•	Doctor –profile v0: required gates pass/fail as expected

Cline Path
	•	locate/import
	•	correlate (dry‑run & persist)
	•	extract –include cline
	•	verify ok, counts include cline

Quality
	•	Idempotency of annotate labels
	•	Deterministic outputs across repeated runs
	•	Negative cases show clear, actionable errors
	•	No leakage of secrets

⸻

5) Suggested Automation Hooks

If you want to codify the above into automated checks:
	•	Add E2E tests similar to the ones described (your repo already includes v0_e2e.rs, replay_annotate_export.rs, etc.). Extend with:
	•	A labels injection test that asserts skipped_lines==0.
	•	A cline‑absent test asserting verify warning includes cline_coverage_zero.
	•	A patch‑failure test crafted by writing an invalid hunk into a copy of a bag.
	•	Add a matrix job in CI to run the core smoke (A1) on Linux and macOS runners.

⸻

6) Pass/Fail Criteria
	•	Pass v0 if:
	•	All Group A and Group C–H tests pass in at least one environment (local or CI).
	•	Group D passes in at least one environment with Cline storage available (or with an emulated storage dir as used in unit tests).
	•	No Group L negative test produces a crash or silent failure (must be clear errors).
	•	No PII/secrets surfaced in Group M checks.
	•	Fail v0 if any required gate (schemas/counts/patches, doctor required checks) is inconsistent or flaky, or label injection shows drift.

⸻

7) Field Notes / Triage Tips
	•	If verify schema errors: capture the first 10 lines reported; attach the offending NDJSON lines to a bug.
	•	If patches_check=false: attach the exact patch file (or a redacted minimal repro).
	•	If cline coverage is 0% unexpectedly:
	•	Run cline locate to confirm storage detection.
	•	Run cline import --since 7d --limit 5 --dry-run.
	•	Ensure your run timestamp overlaps the task created/updated window (tune --pad).

⸻

8) Optional Stretch Cases (Post‑v0)
	•	Multi‑run extract (if/when supported) and verify per sub‑window.
	•	Parquet export (future P1).
	•	More granular cline message overlap metrics.

⸻

Final Note

This plan is intentionally actionable: it mirrors real workflows (dev, analyst, CI), stresses both happy paths and known failure modes, and provides clear, binary pass/fail signals aligned with the v0 definition. If you run through Groups A–H plus the key L‑group negatives, you’ll have high confidence the v0 is implemented correctly end to end.