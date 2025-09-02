use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::{plugin::{Plugin, CommandContext}, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use std::collections::BTreeMap;
use time::format_description::well_known::Rfc3339;

pub struct ReplayPlugin;
impl ReplayPlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanSummary {
    run_id: String,
    count: usize,
    from: String,
    to: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanStep {
    seq: usize,
    patch_path: Option<String>,
    status_path: Option<String>,
    action: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct RunSummary {
    run_id: String,
    branch: String,
    applied: usize,
    dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ExtractSummary {
    run_id: String,
    format: String,          // "dir" | "tgz"
    out: String,             // final artifact (dir or .tar.gz)
    counts: ExtractCounts,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, JsonSchema)]
struct ExtractCounts {
    events_lines: u64,
    steps: usize,
    patches: usize,
    status: usize,
}

impl Plugin for ReplayPlugin {
    fn name(&self) -> &'static str { "replay" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Replay/extract TracePacks from logged runs" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("replay").about(self.describe())
                .subcommand(
                    Command::new("plan").about("Plan steps for a run")
                        .arg(Arg::new("run-id").long("run-id").required(true))
                )
                .subcommand(
                    Command::new("run").about("Apply patches for a run (dry-run by default)")
                        .arg(Arg::new("run-id").long("run-id").required(true))
                        .arg(Arg::new("cwd").long("cwd").value_name("DIR"))
                        .arg(Arg::new("branch").long("branch").value_name("NAME").default_value("replay"))
                        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                )
                .subcommand(
                    Command::new("extract").about("Export a TracePack for a run (events + patches + manifest)")
                        .arg(Arg::new("run-id").long("run-id").required(true))
                        .arg(Arg::new("out").long("out").required(true)
                             .help("If ends with .tar.gz/.tgz -> tarball; otherwise a directory is created"))
                        .arg(Arg::new("include").long("include").value_name("CSV")
                             .help("What to include: events,patches,status,all").default_value("all"))
                )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            Some(("plan", sm)) => {
                let run_id = sm.get_one::<String>("run-id").unwrap().to_string();
                let plan = build_plan(&run_id)?;
                let summary = PlanSummary {
                    run_id,
                    count: plan.count,
                    from: plan.from.clone(),
                    to: plan.to.clone(),
                };
                ctx.ok("replay", "plan", &summary);
                Ok(0)
            }

            Some(("run", sm)) => {
                let run_id = sm.get_one::<String>("run-id").unwrap().to_string();
                let branch = sm.get_one::<String>("branch").unwrap().to_string();
                let dry_run = sm.get_flag("dry-run");
                let _cwd = sm.get_one::<String>("cwd");

                let plan = build_plan(&run_id)?;
                let mut applied = 0;

                if !dry_run {
                    // Apply patches (simplified for demo)
                    for step in &plan.steps {
                        if let Some(patch_path) = &step.patch_path {
                            if std::path::Path::new(patch_path).exists() {
                                applied += 1;
                            }
                        }
                    }
                }

                let summary = RunSummary {
                    run_id,
                    branch,
                    applied,
                    dry_run,
                };
                ctx.ok("replay", "run", &summary);
                Ok(0)
            }

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
                let mut plugin_counts: BTreeMap<String,u64> = Default::default();
                let mut cmd_counts: BTreeMap<String,u64> = Default::default();
                
                if want_events {
                    let mut out = String::new();
                    let ev_root = std::path::Path::new(".nexoscli").join("log");
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
                let mut manifest_status: Vec<serde_json::Value> = vec![];

                if want_patches || want_status {
                    if want_patches { std::fs::create_dir_all(default_stage.join("patches"))?; }
                    if want_status { std::fs::create_dir_all(default_stage.join("status"))?; }
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
                ctx.ok("replay", "extract", &summary);
                Ok(0)
            }

            _ => Ok(ctx.err("replay", "unknown", "replay.unknown", "unknown replay command", None, 2))
        }
    }
}

// --- helpers ---

#[derive(Debug)]
struct Plan {
    count: usize,
    from: String,
    to: String,
    steps: Vec<PlanStep>,
}

fn build_plan(_run_id: &str) -> anyhow::Result<Plan> {
    // Simplified plan building for now - in a real implementation this would
    // parse events and build actual steps
    Ok(Plan {
        count: 0,
        from: "HEAD~1".to_string(),
        to: "HEAD".to_string(),
        steps: vec![],
    })
}

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
