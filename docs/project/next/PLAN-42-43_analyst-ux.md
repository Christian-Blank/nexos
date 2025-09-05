PR‑C (parallel to PR‑A): Plan #42 + #43 — Analyst UX v0

Status: proposal (v0)
Scope labels: replay, observability
Why together: Both changes are read‑only to the core data flow, fully additive, and easy to test in isolation. #42 enriches bags with analysis labels; #43 adds self‑describing dataset cards and a quickstart for analysts.

⸻

0) Objectives
	•	#42 — Labels consistency
	•	Add replay annotate --labels that:
	1.	Upserts a map into bag/manifest.json → manifest.labels.
	2.	Injects the same map into every events.ndjson row under attrs.labels (merge, don’t overwrite).
	•	#43 — Dataset card + Analyst quickstart
	•	On extract (and optionally verify --fix-readme), write a dataset card to bag/README.md with contents, counts, labels, and copy‑paste snippets for Pandas & DuckDB.
	•	Add docs/analysis/QUICKSTART.md with minimal “load & compute” examples.

Non‑Goals
	•	No changes to core event/fileops schemas or emit‑time envelopes.
	•	No Parquet/Arrow export (CSV already covered elsewhere).
	•	No agent behavior changes (this is analyst‑side UX).

⸻

1) User‑visible behavior

New/extended CLI

# Add or update bag-level labels and inject them into each events row
nexoscli replay annotate --bag ./bag --labels exp_id=EXP-001 env=dev treatment=A
# or load from a JSON file (flat object) and optionally override/extend with CLI pairs
nexoscli replay annotate --bag ./bag --from-file labels.json --labels author=qa

# Re-generate dataset card if needed
nexoscli replay verify --bag ./bag --fix-readme

Bag layout (additions highlighted)

bag/
  events.ndjson
  fileops.ndjson
  manifest.json          # labels upserted → manifest.labels
  README.md              # <-- dataset card (#43)
  patches/
  status/
  cline/...

Dataset card (excerpt)

# TracePack: bag-2025-09-05T12-00-00Z

**Created:** 2025-09-05T12:05:10Z

## Contents & Schemas
- `events.ndjson` — nexos/events@0.3
- `fileops.ndjson` — nexos/fileops@0.1

## Counts
{
  "events_lines": 124,
  "fileops": 6,
  "cline_messages": 18
}

## Labels
{
  "exp_id": "EXP-001",
  "treatment": "A",
  "env": "dev"
}

## Tooling
{
  "nexoscli": "nexoscli 0.1.0 (...)",
  "rustc": "rustc 1.80.0 (...)",
  "cargo": "cargo 1.80.0 (...)",
  "os": "linux",
  "arch": "x86_64",
  "git": { "branch": "main", "head": "abc123...", "dirty": false }
}

## Load in Python (Pandas)
```python
import pandas as pd
from pathlib import Path
bag = Path(r"./bag")
events = pd.read_json(bag/"events.ndjson", lines=True)
print(events.head())

Load in DuckDB (SQL)

SELECT * FROM read_json_auto('bag/events.ndjson');

---

## 2) Design

### 2.1 `replay annotate --labels` (post‑hoc enrichment)
- **Input sources**
  - `--labels K=V` pairs (repeatable)
  - `--from-file labels.json` (flat JSON object; merged after file read; CLI pairs win)
- **Manifest upsert**
  - Ensure `manifest.labels` exists; shallow‑merge incoming K/V.
- **Events injection**
  - For every non‑blank line in `events.ndjson`, parse JSON, ensure `attrs` object exists, ensure `attrs.labels` object exists, **merge** labels (CLI wins), write back single‑line JSON.
  - Idempotent: re‑running with the same labels is a no‑op (values are the same).
- **Safety**
  - If `events.ndjson` is missing or a line is invalid JSON, skip that line and continue; return `ok` but include a small `skipped_lines` count in the envelope.

### 2.2 Dataset card writer
- **Hook points**
  - **On extract**: always write `bag/README.md` (replace if exists).
  - **On verify**: add `--fix-readme` to re‑write card from current `manifest.json` and present files.
- **Content**
  - Name (dir name), created timestamp (from `manifest.created_at` or `time_range.to`), present artifacts & **schema IDs**, counts, labels, tooling snapshot (tools/env/git).
  - Language‑agnostic quick loads (Pandas, DuckDB).
- **Deterministic output**: stable ordering of sections; JSON blocks pretty‑printed.

---

## 3) Implementation

### 3.1 CLI surface (replay)
Add to `augment_commands()` under `replay`:
```text
annotate
  --bag <DIR> (required)
  --labels K=V [--labels ...] (optional, repeatable)
  --from-file <JSON> (optional; flat object)

verify
  --bag <DIR>
  --fix-readme (optional; re-write dataset card)

3.2 Helpers (snippets)

Parse K=V list

fn parse_kv_list(items: Option<Vec<String>>) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    if let Some(v) = items {
        for s in v {
            if let Some((k,v)) = s.split_once('=') {
                m.insert(k.trim().into(), serde_json::Value::String(v.trim().into()));
            }
        }
    }
    m
}

Upsert manifest.labels

fn upsert_labels(manifest: &mut serde_json::Value, labels: &serde_json::Map<String, serde_json::Value>) {
    let obj = manifest.as_object_mut().unwrap();
    let lref = obj.entry("labels").or_insert_with(|| serde_json::json!({}));
    let lobj = lref.as_object_mut().unwrap();
    for (k,v) in labels { lobj.insert(k.clone(), v.clone()); }
}

Inject labels into events rows

fn inject_labels_into_events(events_path: &Path, labels: &serde_json::Map<String, serde_json::Value>) -> anyhow::Result<(u64,u64)> {
    let inp = std::fs::read_to_string(events_path)?;
    let mut out = String::with_capacity(inp.len() + 1024);
    let mut total=0u64; let mut skipped=0u64;

    for (i, line) in inp.lines().enumerate() {
        if line.trim().is_empty() { continue; }
        total += 1;
        let mut row: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v, Err(_) => { skipped+=1; continue; }
        };
        let attrs = row.as_object_mut().unwrap()
            .entry("attrs").or_insert_with(|| serde_json::json!({}));
        let l = attrs.as_object_mut().unwrap()
            .entry("labels").or_insert_with(|| serde_json::json!({}));
        let lobj = l.as_object_mut().unwrap();
        for (k,v) in labels { lobj.insert(k.clone(), v.clone()); }
        out.push_str(&serde_json::to_string(&row)?);
        out.push('\n');
    }
    std::fs::write(events_path, out.as_bytes())?;
    Ok((total, skipped))
}

Write dataset card

fn write_dataset_card(bag: &Path, manifest: &serde_json::Value) -> anyhow::Result<()> {
    use std::fmt::Write as _;
    let name = bag.file_name().and_then(|s| s.to_str()).unwrap_or("bag");
    let created = manifest.get("created_at").and_then(|x| x.as_str())
        .or_else(|| manifest.get("time_range").and_then(|t| t.get("to")).and_then(|x| x.as_str()))
        .unwrap_or("-");
    let counts = manifest.get("counts").cloned().unwrap_or(serde_json::json!({}));
    let labels = manifest.get("labels").cloned().unwrap_or(serde_json::json!({}));
    let tools  = manifest.get("tools").cloned().unwrap_or(serde_json::json!({}));
    let env    = manifest.get("env").cloned().unwrap_or(serde_json::json!({}));
    let git    = manifest.get("git").cloned().unwrap_or(serde_json::json!({}));

    let mut md = String::new();
    writeln!(md, "# TracePack: {}\n", name)?;
    writeln!(md, "**Created:** {}", created)?;
    writeln!(md, "\n## Contents & Schemas")?;
    for (file, sch) in [("events.ndjson","nexos/events@0.3"), ("fileops.ndjson","nexos/fileops@0.1")] {
        if bag.join(file).exists() { writeln!(md, "- `{}` — {}", file, sch)?; }
    }
    writeln!(md, "\n## Counts\n```json\n{}\n```", serde_json::to_string_pretty(&counts)?)?;
    writeln!(md, "\n## Labels\n```json\n{}\n```", serde_json::to_string_pretty(&labels)?)?;
    writeln!(md, "\n## Tooling\n```json\n{}\n{}\n{}\n```", 
             serde_json::to_string_pretty(&tools)?, 
             serde_json::to_string_pretty(&env)?, 
             serde_json::to_string_pretty(&git)?)?;
    writeln!(md, "\n## Load in Python (Pandas)\n```python\nimport pandas as pd\nfrom pathlib import Path\nbag = Path(r\"{}\")\nevents = pd.read_json(bag/\"events.ndjson\", lines=True)\nprint(events.head())\n```", bag.display())?;
    writeln!(md, "\n## Load in DuckDB (SQL)\n```sql\nSELECT * FROM read_json_auto('{}');\n```", bag.join("events.ndjson").display())?;
    std::fs::write(bag.join("README.md"), md.as_bytes())?;
    Ok(())
}

3.3 Control flow
	•	On extract success
	•	After manifest.json is written (and counts/tools/env/git populated by PR‑A), call write_dataset_card(&bag, &manifest).
	•	replay annotate command
	1.	Load & merge labels (file → CLI).
	2.	Read manifest.json, upsert_labels, write back (pretty JSON).
	3.	If events.ndjson exists, inject_labels_into_events.
	4.	Return envelope with { bag, labels, total_rows, skipped_lines }.
	•	On replay verify --fix-readme
	•	Read manifest.json; call write_dataset_card.

⸻

4) Files touched
	•	plugins/replay/src/lib.rs — add annotate subcommand + helpers; call write_dataset_card from extract and verify (flag).
	•	docs/analysis/QUICKSTART.md — new.
	•	docs/TRACEPACK.md — mention dataset card and manifest.labels.
	•	nexoscli-bin/tests/v0_labels_card.rs — new E2E test.

(No new third‑party dependencies.)

⸻

5) Testing

Unit
	•	parse_kv_list: empty, single, multiple, whitespace.
	•	upsert_labels: creates section on first run; merges on second; CLI overrides file.
	•	inject_labels_into_events: merges w/o losing existing label keys; counts skipped_lines.

E2E (nexoscli-bin/tests/v0_labels_card.rs)
	1.	Arrange: dev init --yes; create minimal run(s) so events.ndjson exists in a bag:
	•	term run -- bash -lc 'echo hi > a.txt'
	•	Extract: replay extract --run-id ... --out bag1.
	2.	Act: replay annotate --bag bag1 --labels exp_id=EXP-001 --labels treatment=A.
	3.	Assert:
	•	manifest.json contains "labels": { "exp_id": "EXP-001", "treatment":"A" }.
	•	events.ndjson lines contain "attrs":{"labels":{"exp_id":"EXP-001","treatment":"A"...}} (merge semantics).
	4.	Act: replay verify --bag bag1 --fix-readme.
	5.	Assert:
	•	bag1/README.md exists and includes Counts, Labels, Tooling, and Load in Python/SQL sections.

⸻

6) Acceptance criteria (DoD)
	•	replay annotate successfully upserts manifest.labels and injects attrs.labels into each valid events.ndjson row; envelope reports totals and skipped lines.
	•	bag/README.md is created on extract and re‑creatable via verify --fix-readme, containing: Contents & schemas, Counts, Labels, Tooling, and Pandas/DuckDB snippets.
	•	Docs updated: manifest.labels documented; analyst quickstart available.
	•	E2E test passes on CI.

⸻

7) Rollout & compatibility
	•	Completely additive. Existing bags can be annotated post‑hoc; dataset card can be added via verify --fix-readme.
	•	Safe to run multiple times (idempotent merges).

⸻

8) Risks & mitigations
	•	Malformed lines in events.ndjson → we skip and report skipped_lines; do not corrupt file.
	•	Very large events.ndjson → re‑write is O(n) but streaming in memory; file sizes in v0 are modest; optimize later if needed.
	•	Conflicting label keys → last‑writer‑wins (CLI overrides file) is explicit in docs.

⸻

9) Developer checklist
	•	Implement replay annotate (parse, manifest upsert, inject).
	•	Wire dataset card on extract and verify --fix-readme.
	•	Add docs: docs/analysis/QUICKSTART.md, update TRACEPACK.md.
	•	Add E2E test v0_labels_card.rs.
	•	Run cargo build & all tests.

⸻

10) Suggested commit message

feat(replay+docs): PR‑C (#42+#43) — labels annotate + dataset card

- replay annotate --labels: upsert manifest.labels and inject attrs.labels into events.ndjson
- replay extract/verify: write bag/README.md dataset card (counts, labels, tooling, quick loads)
- docs: analysis/QUICKSTART.md and TRACEPACK.md updates
- tests: v0_labels_card e2e


⸻

11) Quick demo

# Create activity and a bag
nexoscli dev init --yes
nexoscli term run -- bash -lc 'echo hi >> demo.txt'
RUN=$(tail -n 1 .nexoscli/log/events.jsonl | jq -r '.run_id')
nexoscli replay extract --run-id "$RUN" --out bag/demo

# Add labels (also injected into every events row)
nexoscli replay annotate --bag bag/demo --labels exp_id=EXP-001 treatment=A env=dev

# Dataset card is already present (from extract); re-generate if needed
nexoscli replay verify --bag bag/demo --fix-readme

# Inspect
cat bag/demo/manifest.json | jq .labels
head -n 1 bag/demo/events.ndjson
sed -n '1,60p' bag/demo/README.md