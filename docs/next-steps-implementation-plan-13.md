Excellent—let’s make TracePacks complete by bundling Cline tasks that occurred during a run’s time window. The goal is portable, analyzable bundles with: events + patches + status + cline (+ manifest). Everything below is drop‑in and keeps your current structure.

⸻

What you’ll get
	•	nexoscli replay extract --include cline ... will now:
	•	Select normalized Cline tasks from .nexoscli/cline/tasks/
	•	Filter them to the run’s time window, with a small safety pad
	•	Copy them into tracepack/cline/tasks/…
	•	Add entries to manifest.json with id, created_at, messages count, size, sha256
	•	Counts updated (counts.cline)
	•	Tarball or directory output still supported

We use mtime as fallback if the JSON lacks timestamps; we apply a ±5 min pad to avoid fencepost misses.

⸻

0) No new crates required

You already have walkdir, sha2, tar, flate2 in workspace/deps.

⸻

1) Extend replay plugin: extract now includes Cline

Open plugins/replay/src/lib.rs and make the following changes.

1.1 Extend counts to track cline

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ExtractCounts {
  events_lines: u64,
  steps: usize,
  patches: usize,
  status: usize,
  cline: usize,            // NEW
}

1.2 Inside the "extract" branch

Find the code that computes first_ts / last_ts, builds the plan, and copies patches/status. After that block (and before writing manifest.json), add the Cline inclusion. The simplest is to paste this entire block and minorly adapt if your local names differ:

// --- 2b) Include Cline tasks (optional) ---
let include_all = include == "all";
let want_cline = include_all || include.contains("cline");
let mut manifest_cline: Vec<serde_json::Value> = vec![];

if want_cline {
    // Resolve time window with pad (±300s) for safety
    let pad = time::Duration::seconds(300);
    let from_ts = first_ts.as_ref().and_then(|s| time::OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t - pad);
    let to_ts   = last_ts.as_ref().and_then(|s| time::OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t + pad);

    let cline_src = std::path::Path::new(".nexoscli").join("cline").join("tasks");
    if cline_src.exists() {
        let dst_root = default_stage.join("cline").join("tasks");
        std::fs::create_dir_all(&dst_root)?;

        for ent in WalkDir::new(&cline_src).min_depth(1).max_depth(2).into_iter().flatten() {
            let p = ent.path();
            if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }

            // Filter by time window (mtime fallback, tolerant)
            let mut in_window = true;
            if let Ok(meta) = std::fs::metadata(p) {
                if let Ok(modified) = meta.modified() {
                    let ts = time::OffsetDateTime::from(modified);
                    if let Some(f) = from_ts { if ts < f { in_window = false; } }
                    if let Some(t) = to_ts   { if ts > t { in_window = false; } }
                }
            }

            if !in_window { continue; }

            // Parse JSON to extract id/created/messages if available
            let raw = std::fs::read(p).unwrap_or_default();
            let v   = serde_json::from_slice::<serde_json::Value>(&raw).unwrap_or(serde_json::json!({}));
            let id  = v.get("id")
                        .or_else(|| v.get("taskId"))
                        .or_else(|| v.get("uuid"))
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("anon-{}", &sha256(&raw)[7..19]));
            let created = v.get("createdAt")
                           .or_else(|| v.pointer("/meta/startedAt"))
                           .or_else(|| v.get("timestamp"))
                           .and_then(|x| x.as_str())
                           .map(|s| s.to_string());
            let msg_count = v.get("messages").and_then(|x| x.as_array()).map(|a| a.len())
                             .or_else(|| v.get("conversation").and_then(|x| x.as_array()).map(|a| a.len()))
                             .or_else(|| v.get("steps").and_then(|x| x.as_array()).map(|a| a.len()))
                             .unwrap_or(0);

            // Destination file name (stable)
            let base = format!("{}-{}.json",
                created.as_deref().unwrap_or("unknown"),
                id.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            );
            let dst = dst_root.join(base);
            std::fs::write(&dst, &raw)?;

            counts.cline += 1;
            manifest_cline.push(serde_json::json!({
                "id": id,
                "dst": format!("cline/tasks/{}", dst.file_name().unwrap().to_string_lossy()),
                "sha256": sha256(&raw),
                "bytes": raw.len(),
                "created_at": created,
                "messages": msg_count
            }));
        }
    }
}

1.3 Add cline to the manifest and counts

Find where you construct manifest:

let manifest = serde_json::json!({
  "version": "0.1",
  "kind": "tracepack",
  "run_id": run_id,
  "created_at": time::OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
  "time_range": { "from": first_ts, "to": last_ts },
  "counts": {
    "events_lines": counts.events_lines,
    "steps": counts.steps,
    "patches": counts.patches,
    "status": counts.status,
    "cline": counts.cline            // NEW
  },
  "git": { "from": plan.from, "to": plan.to },
  "plugins": plugin_counts,
  "commands": cmd_counts,
  "patches": manifest_patches,
  "status_files": manifest_status,
  "cline_tasks": manifest_cline     // NEW
});

Nothing else in the plugin has to change.

⸻

2) Tests (E2E)

Add nexoscli-bin/tests/replay_extract_cline.rs:

mod helpers; use helpers::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn replay_extract_includes_cline_tasks() {
    let t = tmp();

    // 1) make a run (generates events)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "one\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    let j = run_json(t.path(), &["term","run","echo","hello"]);
    assert_eq!(j["errors"], serde_json::Value::Array(vec![]));
    let run_id = j["meta"]["run_id"].as_str().unwrap().to_string();

    // 2) synthesize a normalized cline task in the default location
    let tasks_dir = t.path().join(".nexoscli").join("cline").join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    let task = serde_json::json!({
        "id": "task-123",
        "createdAt": "2025-09-01T12:00:00Z",
        "messages": [ {"role":"user","content":"do x"}, {"role":"assistant","content":"ok"} ]
    });
    fs::write(tasks_dir.join("task-123.json"), serde_json::to_vec_pretty(&task).unwrap()).unwrap();

    // 3) extract with cline included (dir)
    let outdir = t.path().join("packs").join("with-cline");
    let v = run_json(t.path(), &["replay","extract","--run-id",&run_id,"--out",outdir.to_str().unwrap(),"--include","cline"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    assert!(outdir.join("manifest.json").exists());

    // verify cline files present
    let cline_dir = outdir.join("cline").join("tasks");
    assert!(cline_dir.exists());
    let mut found = false;
    for ent in fs::read_dir(&cline_dir).unwrap() {
        let p = ent.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) == Some("json") {
            let body = fs::read_to_string(&p).unwrap();
            if body.contains("\"task-123\"") { found = true; }
        }
    }
    assert!(found, "expected cline task copied into tracepack");

    // manifest contains cline count
    let m: serde_json::Value = serde_json::from_str(&fs::read_to_string(outdir.join("manifest.json")).unwrap()).unwrap();
    assert!(m["counts"]["cline"].as_u64().unwrap_or(0) >= 1);
    assert!(m["cline_tasks"].as_array().unwrap().len() >= 1);
}

Run all tests:

cargo test --workspace


⸻

3) Docs update

Append to docs/TRACEPACK.md:

## Cline Tasks (optional)

If you pass `--include cline` (or `--include all`), the extractor selects normalized Cline tasks
from `.nexoscli/cline/tasks/` whose modified time falls within the run’s time window (±5 minutes),
and copies them into:

cline/tasks/-.json

The `manifest.json` includes an array `cline_tasks` with:
- `id`, `dst`, `sha256`, `bytes`, `created_at`, `messages`
and a count at `counts.cline`.

**Example:**
```bash
nexoscli replay extract --run-id RUN-... --include cline --out tracepack.tgz

Tip: run nexoscli cline tail --exit-after 10 while working to keep normalized tasks fresh.

Rebuild your generated docs if you want:

```bash
nexoscli dev docs build


⸻

4) Quick manual acceptance

# 1) Produce some events + a patch
git init && echo hi > a.txt && git add . && git commit -m init
nexoscli term run -- sh -lc "echo more >> a.txt" --output json | tee /tmp/run.json
RUN=$(jq -r '.meta.run_id' /tmp/run.json)

# 2) (Optional) ensure you have normalized cline tasks
# If you’ve been using Cline with tailing:
#   nexoscli cline tail --exit-after 3
# Or drop a test task:
mkdir -p .nexoscli/cline/tasks
echo '{"id":"demo-1","createdAt":"2025-09-01T12:00:00Z","messages":[1,2,3]}' > .nexoscli/cline/tasks/demo-1.json

# 3) Extract a tarball with cline
nexoscli replay extract --run-id "$RUN" --include cline --out tracepack.tgz
tar tzf tracepack.tgz | grep -E 'manifest.json|cline/tasks' | head


⸻

Design choices (why this is stable)
	•	Time‑windowed inclusion with a ±5 min pad ensures tasks adjacent to the run get captured even with minor clock skew or write delays.
	•	Id + sha256 + size in manifest.json make integrity check trivial.
	•	No orchestration coupling: we use normalized files you already write (.nexoscli/cline/tasks/), not Cline internals.
	•	Deterministic names: <createdAt>-<id>.json (or a stable hash fallback) → sortability and de‑duplication.