---
status: in-progress
plan_id: '40'
slug: 41_env-and-repo
task_id: TASK-20250905-create-some-activity
started_at: 2025-09-05T21:32:07.008949Z
source: next
---
Plan #40 + #41 — Env & Repo Snapshot + Verify++ & Doctor v0

Status: proposal (v0)
Scope labels: replay, dev-plugin, observability
Goal date: v0 branch landing

⸻

0) Why this matters

For our TracePack (bag) v0 to be analyzable and auditable, every bag must include (a) deterministic environment context (tool versions, OS/arch, repo state) and (b) a strong verifier + CI profile that catches schema drift, count mismatches, broken patches, and missing agent coverage. This plan combines:
	•	#40 Env & Repo Snapshot (P0) — write tools, env, git into every bag’s manifest.json.
	•	#41 Verify++ & Doctor v0 profile (P0) — robust replay verify and a dev doctor --profile v0 that sets sane default gates.

The combined change is additive, low‑risk, and sits entirely in plugins/replay and plugins/dev plus docs/tests.

⸻

1) Goals & Non‑Goals

Goals
	•	Add deterministic context to manifest.json: tool versions, OS/arch, git branch/HEAD/dirty.
	•	Strengthen replay verify:
	•	Validate events and fileops against canonical JSON Schemas (from #34).
	•	Check counts vs manifest.counts.
	•	Patch dry‑run: git apply --check for patches/*.patch (if present).
	•	Agent coverage warning: flag missing Cline coverage when term.run exists.
	•	Introduce dev doctor --profile v0: consistent CI gates for v0.

Non‑Goals
	•	No Parquet exports, no emit‑time label injection (covered elsewhere).
	•	No changes to schema definitions (reuse #34).
	•	No new cross‑crate deps (replay/dev only; cline used only via files).

⸻

2) User‑visible behavior

New/extended CLI
	•	nexoscli replay extract ...
→ Bags now include manifest.tools, manifest.env, manifest.git and updated manifest.counts.
	•	nexoscli replay verify --bag <DIR>
→ Validates schemas, counts, patch apply checks, and emits a warning if Cline coverage is zero while term.run is present.
	•	nexoscli dev doctor --ci --profile v0
→ Preselects sensible gates; shows Cline correlation coverage as info.

Manifest example (additions highlighted)

{
  "schema": "nexos/tracepack@0.1",
  "run_id": "RUN-...",
  "time_range": { "from": "...", "to": "..." },
  "counts": {
    "events_lines": 124,
    "fileops": 6,
    "cline_messages": 18
  },
  "tools": {                   // <-- #40
    "nexoscli": "nexoscli 0.1.0 (rev ...)",
    "rustc": "rustc 1.80.0 (..)",
    "cargo": "cargo 1.80.0 (..)"
  },
  "env": {                     // <-- #40
    "os": "linux",
    "arch": "x86_64"
  },
  "git": {                     // <-- #40
    "branch": "main",
    "head": "abc1234...",
    "dirty": false
  }
}

Verify result (shape)

{
  "data": {
    "bag": "bag/demo",
    "ok": true,
    "events": { "ok": true, "lines": 124, "invalid": 0, "errors": [] },
    "fileops": { "ok": true, "lines": 6, "invalid": 0, "errors": [] },
    "counts_match": { "events_lines": true, "fileops": true },
    "patches_check": true,
    "cline_coverage": { "had_term_run": true, "has_cline_artifacts": true, "warning": null }
  }
}


⸻

3) Design

3.1 Env & Repo Snapshot (extract‑time)
	•	Tools snapshot: best‑effort rustc --version, cargo --version, and nexoscli --version by re‑invoking the current binary.
	•	Env snapshot: std::env::consts::{OS, ARCH}.
	•	Git snapshot: git rev-parse --abbrev-ref HEAD, git rev-parse HEAD, and git diff --quiet for dirty flag (worktree best‑effort).
	•	Counts: during extract, compute and write:
	•	counts.events_lines (non‑empty lines in events.ndjson),
	•	counts.fileops (non‑empty lines in fileops.ndjson if present),
	•	counts.cline_messages (lines in cline/messages.ndjson if present).

All snapshots and counts are non‑fatal: failures record null/omit and extract still succeeds.

3.2 Verify++
	•	Schema validation: reuse #34 schemas::get_schema_json("events","0.3") and "fileops","0.1".
	•	Counts check: recompute line counts and compare to manifest.counts. If missing, report but don’t fail the bag (warn only).
	•	Patch dry‑run: for each patches/*.patch, run git apply --check with the patch piped to stdin. Aggregate boolean across files.
	•	Cline coverage guard (warn‑only):
	•	Detect if any term.run lines exist in events.ndjson.
	•	Consider “has Cline” when either bag/cline/tasks/* exists or bag/cline/messages.ndjson has at least one line.
	•	If term.run and no Cline: add "warnings": ["cline_coverage_zero"].

3.3 Doctor v0 Profile
	•	--profile v0 expands wanted checks to:
graph, plugins, config, events, tools, cline, rules
	•	Required gates (fail build if red):
graph_check, plugins_check, event_log_writable
	•	Cline: compute correlation coverage over last 24h (info severity).
e.g., coverage=85% over 27 runs.
	•	Rules: informational in v0 (non‑required), so Rules v1 can land in parallel.

⸻

4) Implementation steps

Step A — replay extract: capture & persist snapshots (+ counts)

File: plugins/replay/src/lib.rs (or split into util.rs if preferred)
	1.	Helpers
	•	capture_cmd_output(cmd, args) -> Option<String>
	•	collect_tools_snapshot() -> serde_json::Value
	•	collect_env_snapshot() -> serde_json::Value
	•	collect_git_snapshot(repo_root: &Path) -> serde_json::Value
	•	count_lines(path) -> u64 (skip blank)
	2.	Inject into manifest
After building the manifest skeleton and writing base artifacts:

manifest["tools"] = collect_tools_snapshot();
manifest["env"]   = collect_env_snapshot();
manifest["git"]   = collect_git_snapshot(&repo_root);

// counts
let mut counts = manifest["counts"].take();
upsert(&mut counts, "events_lines", count_lines(bag/"events.ndjson"));
if (bag/"fileops.ndjson").exists() { upsert(&mut counts, "fileops", count_lines(bag/"fileops.ndjson")); }
if (bag/"cline/messages.ndjson").exists() { upsert(&mut counts, "cline_messages", count_lines(bag/"cline/messages.ndjson")); }
manifest["counts"] = counts;



Behavior: All snapshots are best‑effort (Options). Extract never fails on missing tools/git.

⸻

Step B — replay verify: schema, counts, patches, cline coverage

File: plugins/replay/src/lib.rs
	1.	Use #34 schemas

let events_schema = schemas::get_schema_json("events","0.3").unwrap();
let fileops_schema = schemas::get_schema_json("fileops","0.1").unwrap();


	2.	Validate NDJSON (streaming) → per‑artifact report: { ok, lines, invalid, errors[] }.
	3.	Counts check
	•	recompute events_lines/fileops and compare to manifest.counts.
	•	populate counts_match booleans; do not fail solely on missing counts fields.
	4.	Patch dry‑run
	•	If patches/*.patch exists, run git apply --check. Aggregate to patches_check.
	5.	Cline coverage
	•	Compute had_term_run, has_cline_artifacts, warning.
	6.	Exit code
	•	ok = events.ok && fileops.ok && patches_check
	•	counts mismatch and cline coverage are warnings unless explicitly configured otherwise later.

⸻

Step C — dev doctor --profile v0

File: plugins/dev/src/lib.rs
	•	Extend CLI: --profile v0
	•	Resolve wanted checks and required gates per §3.3.
	•	Cline section:
	•	cline locate (if configured)
	•	cline correlate --since 24h --pad 5m --dry-run
	•	Report coverage as info; provide actionable fix if storage missing.

Note: Keep rules as info in v0 to decouple Rules v1 rollout.

⸻

(Optional) Step D — dev env print helper

File: plugins/dev/src/lib.rs
Add dev env print returning the exact JSON we write into the bag for quick debugging. (Optional but handy; safe to skip if time‑boxed.)

⸻

5) Files touched
	•	plugins/replay/Cargo.toml — (no new deps for #40; #41 reuses jsonschema from #34)
	•	plugins/replay/src/lib.rs — helpers, extract augmentation, verify++
	•	plugins/replay/src/schemas.rs — reuse (from #34)
	•	plugins/dev/src/lib.rs — doctor --profile v0 (+ optional dev env print)
	•	docs/TRACEPACK.md — add manifest.tools/env/git; verify behaviors
	•	nexoscli-bin/tests/v0_e2e.rs — E2E acceptance for #40+41

⸻

6) Testing strategy

Unit
	•	Snapshot helpers: tolerate missing binaries; return null without panicking.
	•	Counts: correct with blank‑line filtering.
	•	Verify: schema invalid line → invalid > 0, errors include line no.

Integration / E2E

File: nexoscli-bin/tests/v0_e2e.rs
	1.	dev init → two term run invocations to generate events & patches.
	2.	replay extract --run-id ... --out bag/
	•	manifest.json contains tools/env/git and counts.*.
	3.	replay verify --bag bag/
	•	ok=true, patches_check=true, counts_match.*=true, cline info present (warning only if missing).
	4.	dev doctor --ci --profile v0
	•	Exit code success; cline check shows coverage summary.

(We already have prior E2Es; this one combines the behaviors.)

⸻

7) Acceptance criteria (DoD)
	•	Bags created by replay extract include manifest.tools, manifest.env, manifest.git, and counts.events_lines (+ fileops if present; cline_messages if present).
	•	replay verify:
	•	Validates events (nexos/events@0.3) and fileops (nexos/fileops@0.1) using #34 schemas.
	•	Compares counts and reports counts_match.
	•	Performs git apply --check for patches/*.patch when present.
	•	Emits warning cline_coverage_zero when term.run exists and no Cline artifacts are present.
	•	dev doctor --profile v0:
	•	Uses required gates: graph_check, plugins_check, event_log_writable.
	•	Reports cline coverage (info) for last 24h.
	•	Exits success on a healthy repo.
	•	Docs updated (TRACEPACK) to describe the new manifest fields and verify behavior.
	•	E2E test passes on CI.

⸻

8) Rollout & compatibility
	•	Purely additive: no migrations required; older bags still verify (counts checks degrade to “missing field” warnings).
	•	Extract is best‑effort; lack of git/rustc doesn’t block bag creation.
	•	Doctor v0 keeps gates narrow to avoid blocking Rules v1 migration.

⸻

9) Risks & mitigations
	•	git apply --check flakiness on non‑git dirs → run in a temp repo context or pipe stdin without cwd coupling; treat failures as a verify failure only when patches exist.
	•	Tool discovery failures → captured as null, never fatal.
	•	Counts drift due to blank lines → explicitly skip empty lines in both extract and verify.

⸻

10) Developer checklist (cut‑and‑merge)
	•	Implement replay snapshot helpers; write to manifest.
	•	Compute and write counts during extract.
	•	Wire verify++ using #34 schemas module.
	•	Add patch --check logic; aggregate boolean.
	•	Add cline coverage warning logic.
	•	Add doctor --profile v0 (wanted/required sets; cline coverage info).
	•	Update TRACEPACK docs.
	•	Add v0_e2e.rs test; ensure green locally and in CI.

⸻

11) Suggested commit message

feat(replay+dev): Plan #40+#41 — Env/Repo snapshot in bags, Verify++, and Doctor v0

- replay extract: write manifest.tools/env/git and counts (events_lines,fileops,cline_messages)
- replay verify: schema validation (events/fileops), counts check, git apply --check for patches,
  cline coverage warn-when-zero
- dev doctor: --profile v0 (graph/plugins/event-log required gates) and cline coverage (info)
- docs: TracePack manifest additions & verify behaviors
- tests: v0_e2e extract→verify→doctor


⸻

12) Appendix — CLI quick demo (expected)

# Create some activity
nexoscli dev init --yes
nexoscli term run -- bash -lc 'git init && git config user.email t@e && git config user.name T && echo hi> a && git add a && git commit -m init'
RUN=$(tail -n 1 .nexoscli/log/events.jsonl | jq -r '.run_id')

# Extract → manifest contains tools/env/git/counts
nexoscli replay extract --run-id "$RUN" --out bag/demo

# Verify (schema, counts, patches, cline coverage)
nexoscli replay verify --bag bag/demo --output json

# Doctor (profile v0)
nexoscli dev doctor --ci --profile v0 --output json


⸻
