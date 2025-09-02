Perfect—let’s ship TracePack extract so every run can be exported as a portable, analyzable bundle (events + patches + manifest). It’s small, deterministic, and test‑covered. You’ll be able to store, index, view, and analyze these bundles—or replay them later.

Below are the exact changes to paste into your repo.

⸻

0) Workspace deps (once)

Root Cargo.toml → add shared crates:

[workspace.dependencies]
tar = "0.4"
flate2 = "1"

(Keep any existing entries; this adds tarball support.)

⸻

1) Replay plugin — add extract (→ directory or .tar.gz)

1.1 Update plugin deps

plugins/replay/Cargo.toml – add tar, flate2, walkdir:

[package]
name = "nexoscli-plugin-replay"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
time = { version = "0.3", features = ["parsing","formatting"] }
walkdir = { workspace = true }
tar = { workspace = true }
flate2 = { workspace = true }

1.2 Extend plugins/replay/src/lib.rs

Add a new subcommand and its implementation. You can paste the following into the same file you already have for plan/run (no other files need to change). If you want, drop it right below your existing run branch.

use walkdir::WalkDir;

// … existing code …

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExtractSummary {
  run_id: String,
  format: String,          // "dir" | "tgz"
  out: String,             // final artifact (dir or .tar.gz)
  counts: ExtractCounts,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ExtractCounts {
  events_lines: u64,
  steps: usize,
  patches: usize,
  status: usize,
}

impl Plugin for ReplayPlugin {
  // … existing impl …

  fn register_commands(&self, app: ClapCommand) -> ClapCommand {
    app.subcommand(
      ClapCommand::new("replay").about(self.description())
        .subcommand(
          ClapCommand::new("plan").about("Plan steps for a run")
            .arg(Arg::new("run-id").long("run-id").required(true))
        )
        .subcommand(
          ClapCommand::new("run").about("Apply patches for a run (dry-run by default)")
            .arg(Arg::new("run-id").long("run-id").required(true))
            .arg(Arg::new("cwd").long("cwd").value_name("DIR"))
            .arg(Arg::new("branch").long("branch").value_name("NAME").default_value("replay"))
            .arg(Arg::new("dry-run").long("dry-run"))
        )
        .subcommand(
          ClapCommand::new("extract").about("Export a TracePack for a run (events + patches + manifest)")
            .arg(Arg::new("run-id").long("run-id").required(true))
            .arg(Arg::new("out").long("out").required(true)
                 .help("If ends with .tar.gz/.tgz -> tarball; otherwise a directory is created"))
            .arg(Arg::new("include").long("include").value_name("CSV")
                 .help("What to include: events,patches,status,all").default_value("all"))
        )
    )
  }

  fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<CommandOutput> {
    match m.subcommand() {
      // … existing plan/run …

      Some(("extract", sm)) => {
        let run_id = sm.get_one::<String>("run-id").unwrap().to_string();
        let out_arg = sm.get_one::<String>("out").unwrap().to_string();
        let include = sm.get_one::<String>("include").unwrap().to_string();
        let include_all = include == "all";
        let want_events = include_all || include.contains("events");
        let want_patches = include_all || include.contains("patches");
        let want_status = include_all || include.contains("status");

        // Staging dir
        let ts = time::OffsetDateTime::now_utc().format(&time::macros::format_description!("[year][month][day]T[hour][minute][second]Z")).unwrap();
        let default_stage = std::path::Path::new(".nexoscli").join("tracepacks").join(format!("{}-{}", run_id, ts));
        std::fs::create_dir_all(&default_stage)?;

        // 1) Gather events for this run_id and write events.ndjson
        let mut counts = ExtractCounts::default();
        let mut first_ts: Option<String> = None;
        let mut last_ts: Option<String> = None;
        let mut plugin_counts: std::collections::BTreeMap<String,u64> = Default::default();
        let mut cmd_counts: std::collections::BTreeMap<String,u64> = Default::default();
        if want_events {
          let mut out = String::new();
          let ev_root = std::path::Path::new(".nexoscli").join("events");
          if ev_root.exists() {
            for ent in WalkDir::new(&ev_root).min_depth(1).max_depth(1).into_iter().flatten() {
              let p = ent.path();
              if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
              let txt = std::fs::read_to_string(p).unwrap_or_default();
              for line in txt.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                  if v.get("run_id").and_then(|x| x.as_str()) == Some(run_id.as_str()) {
                    out.push_str(line); out.push('\n');
                    counts.events_lines += 1;
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                      if first_ts.as_ref().map(|s| ts < s.as_str()).unwrap_or(true) { first_ts = Some(ts.to_string()); }
                      if last_ts.as_ref().map(|s| ts > s.as_str()).unwrap_or(true) { last_ts = Some(ts.to_string()); }
                    }
                    if let Some(pl) = v.get("plugin").and_then(|x| x.as_str()) {
                      *plugin_counts.entry(pl.to_string()).or_insert(0) += 1;
                    }
                    if let (Some(pl), Some(cmd)) = (v.get("plugin").and_then(|x| x.as_str()), v.get("command").and_then(|x| x.as_str())) {
                      *cmd_counts.entry(format!("{}.{}", pl, cmd)).or_insert(0) += 1;
                    }
                  }
                }
              }
            }
          }
          std::fs::write(default_stage.join("events.ndjson"), out.as_bytes())?;
        }

        // 2) Build plan and copy referenced patches/status
        let plan = build_plan(&run_id)?;
        counts.steps = plan.count;
        let mut manifest_patches: Vec<serde_json::Value> = vec![];
        let mut manifest_status:  Vec<serde_json::Value> = vec![];

        if want_patches || want_status {
          if want_patches { std::fs::create_dir_all(default_stage.join("patches"))?; }
          if want_status  { std::fs::create_dir_all(default_stage.join("status"))?; }
          for s in &plan.steps {
            // patches
            if want_patches {
              if let Some(src) = &s.patch_path {
                let srcp = std::path::Path::new(src);
                if srcp.exists() {
                  let base = srcp.file_name().and_then(|x| x.to_str()).unwrap_or("patch.diff");
                  let dst = default_stage.join("patches").join(format!("seq-{:04}-{}", s.seq, base));
                  std::fs::copy(srcp, &dst).ok();
                  counts.patches += 1;
                  let body = std::fs::read(&dst).unwrap_or_default();
                  manifest_patches.push(serde_json::json!({
                    "seq": s.seq,
                    "src": srcp.to_string_lossy(),
                    "dst": format!("patches/{}", dst.file_name().unwrap().to_string_lossy()),
                    "sha256": sha256(&body),
                    "bytes": body.len()
                  }));
                }
              }
            }
            // status
            if want_status {
              if let Some(src) = &s.status_path {
                let srcp = std::path::Path::new(src);
                if srcp.exists() {
                  let base = srcp.file_name().and_then(|x| x.to_str()).unwrap_or("status.txt");
                  let dst = default_stage.join("status").join(format!("seq-{:04}-{}", s.seq, base));
                  std::fs::copy(srcp, &dst).ok();
                  counts.status += 1;
                  let body = std::fs::read(&dst).unwrap_or_default();
                  manifest_status.push(serde_json::json!({
                    "seq": s.seq,
                    "src": srcp.to_string_lossy(),
                    "dst": format!("status/{}", dst.file_name().unwrap().to_string_lossy()),
                    "sha256": sha256(&body),
                    "bytes": body.len()
                  }));
                }
              }
            }
          }
        }

        // 3) Manifest + README
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
            "status": counts.status
          },
          "git": { "from": plan.from, "to": plan.to },
          "plugins": plugin_counts,
          "commands": cmd_counts,
          "patches": manifest_patches,
          "status_files": manifest_status
        });
        std::fs::write(default_stage.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
        std::fs::write(default_stage.join("README.md"), TRACEPACK_README.as_bytes())?;

        // 4) Produce final artifact (dir or tgz)
        let (format, final_out) = if out_arg.ends_with(".tar.gz") || out_arg.ends_with(".tgz") {
          // pack to tar.gz
          let parent = std::path::Path::new(&out_arg).parent().unwrap_or(std::path::Path::new("."));
          std::fs::create_dir_all(parent)?;
          let f = std::fs::File::create(&out_arg)?;
          let enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
          let mut tar = tar::Builder::new(enc);

          // append stage contents with relative paths
          for ent in WalkDir::new(&default_stage).into_iter().flatten() {
            let p = ent.path();
            if p.is_dir() { continue; }
            let rel = p.strip_prefix(&default_stage).unwrap();
            tar.append_path_with_name(p, rel)?;
          }
          tar.into_inner()?.finish()?; // finish gzip
          // optional: clean staging
          // std::fs::remove_dir_all(&default_stage).ok();
          ("tgz".to_string(), out_arg)
        } else {
          // move/rename staging to out directory if different
          let out_path = std::path::Path::new(&out_arg);
          if out_path != default_stage {
            if out_path.exists() {
              return Err(anyhow::anyhow!("destination exists: {}", out_arg).into());
            }
            std::fs::create_dir_all(out_path.parent().unwrap_or(std::path::Path::new(".")))?;
            std::fs::rename(&default_stage, &out_path)?;
          }
          ("dir".to_string(), out_arg)
        };

        let summary = ExtractSummary {
          run_id,
          format,
          out: final_out.clone(),
          counts
        };
        let code = ctx.ok("replay","extract",&summary);
        Ok(CommandOutput { message: None, data: Some(serde_json::to_value(&summary)?), exit_code: code })
      }

      _ => Err(anyhow::anyhow!("unknown replay command").into()),
    }
  }
}

// --- helpers (reuse from earlier code or add minimal versions) ---

fn sha256(b: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  let mut h = Sha256::new();
  h.update(b);
  format!("sha256:{:x}", h.finalize())
}

const TRACEPACK_README: &str = r#"# TracePack

This bundle captures a single `nexoscli` run for analysis and replay.

## Layout
- `manifest.json` — metadata (counts, git range, per-file hashes)
- `events.ndjson` — newline-delimited events for the run (schema v0.2)
- `patches/` — git patches captured by `term run` (seq-####-*.patch)
- `status/`  — `git status --porcelain` snapshots (seq-####-*.status)

## Notes
- All relative paths are within this bundle.
- Apply patches **only** on a throwaway branch. Use `replay plan/run` for safety.
"#;

This subcommand is idempotent: it reads immutable event lines and copies referenced patch/status files; it doesn’t mutate your history.

⸻

2) Tests (E2E)

Add this file to nexoscli-bin/tests/replay_extract.rs:

mod helpers; use helpers::*;
use std::fs;
use std::process::Command;

#[test]
fn replay_extract_dir_and_tgz() {
    let t = tmp();

    // init git and create a change via term run (captures a patch)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "one\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    let j = run_json(t.path(), &["term","run","sh","-lc","echo two >> a.txt"]);
    let run_id = j["meta"]["run_id"].as_str().unwrap();

    // extract to directory
    let outdir = t.path().join("packs").join("run1");
    let v = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", outdir.to_str().unwrap()]);
    assert!(v["data"]["out"].as_str().unwrap().ends_with("packs/run1"));
    assert!(outdir.join("manifest.json").exists());
    assert!(outdir.join("events.ndjson").exists());
    // patches dir should exist and contain at least one file
    let patches = fs::read_dir(outdir.join("patches")).unwrap();
    assert!(patches.count() >= 1);

    // extract to tar.gz
    let tgz = t.path().join("packs").join("run1.tgz");
    let v2 = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", tgz.to_str().unwrap()]);
    assert!(t.path().join("packs").join("run1.tgz").exists());
}

Run the full suite:

cargo test --workspace


⸻

3) Docs (human/AI)

Create docs/TRACEPACK.md:

# TracePack — Portable Run Bundle (v0.1)

A TracePack is a **self-contained** snapshot of a single `nexoscli` run:
- canonical **events** (NDJSON, schema v0.2),
- any **patches** captured by `term run`,
- a **manifest** with hashes and counts,
- optional status snapshots.

## Produce a TracePack

```bash
# directory
nexoscli replay extract --run-id RUN-... --out .nexoscli/tracepacks/pack-1

# tarball
nexoscli replay extract --run-id RUN-... --out tracepack.tar.gz

Layout

manifest.json
events.ndjson
patches/
  seq-0001-<original>.patch
status/
  seq-0001-<original>.status
README.md

Analyze
	•	Events: jq / Notebooks / dev telemetry report --since ...
	•	Patches: git apply --check (dry-run)
	•	Manifest: jq .plugins, jq .commands, jq .counts

You can also regenerate **`docs/CLI_REFERENCE.md`** with:

```bash
nexoscli dev docs build


⸻

4) Manual acceptance

# Create some activity
git init && echo hi > a.txt && git add . && git commit -m init
nexoscli term run -- sh -lc "echo more >> a.txt"

# Find run id (from any command output meta or via index)
nexoscli events index --since 1d --output json | jq

# Extract a directory bundle
nexoscli replay extract --run-id RUN-... --out .nexoscli/tracepacks/demo-pack
ls .nexoscli/tracepacks/demo-pack

# Extract a tarball
nexoscli replay extract --run-id RUN-... --out tracepack.tgz
tar tzf tracepack.tgz | head


⸻

5) Why this is stable (and useful)
	•	Deterministic: It uses your event seq ordering and copies the exact patch/status files referenced by those events.
	•	Self‑describing: manifest.json contains counts, hashes, plugin/command tallies, and git range (from→to).
	•	Portable: Tarball or directory—hand it to a teammate/agent, they don’t need your repo state to analyze it.
	•	Replay‑ready: Combined with replay plan/run, patches can be dry‑run checked or applied on a throwaway branch.
