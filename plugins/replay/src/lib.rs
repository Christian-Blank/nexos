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
    cline: usize,            // NEW
}

// --------- NEXBags (TracePack v0.2) types ---------

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagBuildSummary {
    run_id: String,
    format: String, // dir|tgz
    out: String,
    counts: BagCounts,
    time_range: Option<BagTimeRange>,
    chain_ok: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, JsonSchema)]
struct BagCounts {
    events: u64,
    term_commands: u64,
    files_watch: u64,
    cline_tasks: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagTimeRange {
    start: String,
    end: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagVerifyResult {
    ok: bool,
    chain_ok: bool,
    count: u64,
    first_seq: Option<u64>,
    last_seq: Option<u64>,
    issues: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagInfoSummary {
    ok: bool,
    format: String,
    topics: BTreeMap<String, serde_json::Value>,
    versions: BTreeMap<String, String>,
    counts: BagCounts,
    time_range: Option<BagTimeRange>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct CatSummary {
    ok: bool,
    topic: String,
    lines: u64,
    bytes: u64,
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
                             .help("What to include: events,patches,status,cline,all").default_value("all"))
                )
                .subcommand(
                    Command::new("bag").about("TracePack v0.2 bag operations")
                        .subcommand(
                            Command::new("build").about("Build a NEXBag (TracePack v0.2)")
                                .arg(Arg::new("run-id").long("run-id").required(true))
                                .arg(Arg::new("out").long("out").required(true)
                                     .help("Directory or .tgz/.tar.gz"))
                        )
                        .subcommand(
                            Command::new("verify").about("Verify bag integrity (event chain)")
                                .arg(Arg::new("bag").long("bag").required(true).help("Path to bag dir or .tgz/.tar.gz"))
                        )
                        .subcommand(
                            Command::new("info").about("Show bag summary and counts")
                                .arg(Arg::new("bag").long("bag").required(true))
                        )
                        .subcommand(
                            Command::new("cat").about("Stream a topic from the bag")
                                .arg(Arg::new("bag").long("bag").required(true))
                                .arg(Arg::new("topic").long("topic").required(true).value_parser(["events","term/commands","files/watch"]))
                                .arg(Arg::new("lines").long("lines").value_name("N").help("Max lines to print").num_args(1))
                        )
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
                let want_cline = include_all || include.contains("cline");

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

                // 2b) Include Cline tasks (optional)
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

            Some(("bag", sm)) => {
                match sm.subcommand() {
                    Some(("build", bsm)) => {
                        let run_id = bsm.get_one::<String>("run-id").unwrap().to_string();
                        let out_arg = bsm.get_one::<String>("out").unwrap().to_string();
                        // staging
                        let ts = time::OffsetDateTime::now_utc().format(&time::macros::format_description!("[year][month][day]T[hour][minute][second]Z")).unwrap();
                        let stage = std::path::Path::new(".nexoscli").join("tracepacks").join(format!("bag-{}-{}", run_id, ts));
                        std::fs::create_dir_all(stage.join("topics")).ok();

                        // Prefer per-run events file
                        let run_file = std::path::Path::new(".nexoscli").join("log").join("runs").join(format!("{}.jsonl", run_id));
                        let (mut first_ts, mut last_ts): (Option<String>, Option<String>) = (None, None);
                        let mut events_count: u64 = 0;

                        let events_text = if run_file.exists() {
                            let txt = std::fs::read_to_string(&run_file).unwrap_or_default();
                            for line in txt.lines() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                                        if first_ts.as_ref().map(|s| ts < s.as_str()).unwrap_or(true) { first_ts = Some(ts.to_string()); }
                                        if last_ts.as_ref().map(|s| ts > s.as_str()).unwrap_or(true) { last_ts = Some(ts.to_string()); }
                                    }
                                    events_count += 1;
                                }
                            }
                            txt
                        } else {
                            // Fallback: filter global logs
                            let ev_root = std::path::Path::new(".nexoscli").join("log");
                            let mut out = String::new();
                            if ev_root.exists() {
                                for ent in WalkDir::new(&ev_root).min_depth(1).max_depth(1).into_iter().flatten() {
                                    let p = ent.path();
                                    if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
                                    let txt = std::fs::read_to_string(p).unwrap_or_default();
                                    for line in txt.lines() {
                                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                                            if v.get("run_id").and_then(|x| x.as_str()) == Some(run_id.as_str()) {
                                                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                                                    if first_ts.as_ref().map(|s| ts < s.as_str()).unwrap_or(true) { first_ts = Some(ts.to_string()); }
                                                    if last_ts.as_ref().map(|s| ts > s.as_str()).unwrap_or(true) { last_ts = Some(ts.to_string()); }
                                                }
                                                out.push_str(line); out.push('\n');
                                                events_count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                            out
                        };
                        std::fs::write(stage.join("topics").join("events.ndjson"), events_text.as_bytes())?;

                        // Copy run manifest if present
                        let run_manifest_src = std::path::Path::new(".nexoscli").join("log").join("runs").join(format!("{}.manifest.json", run_id));
                        if run_manifest_src.exists() {
                            let raw = std::fs::read(&run_manifest_src).unwrap_or_default();
                            std::fs::write(stage.join("run_manifest.json"), &raw)?;
                        }

                        // Include cline tasks by time window (±5m)
                        let mut cline_count: u64 = 0;
                        if first_ts.is_some() && last_ts.is_some() {
                            let pad = time::Duration::seconds(300);
                            let from_ts = first_ts.as_ref().and_then(|s| time::OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t - pad);
                            let to_ts   = last_ts.as_ref().and_then(|s| time::OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t + pad);
                            let cline_src = std::path::Path::new(".nexoscli").join("cline").join("tasks");
                            if cline_src.exists() {
                                let dst_root = stage.join("cline").join("tasks");
                                std::fs::create_dir_all(&dst_root).ok();
                                for ent in WalkDir::new(&cline_src).min_depth(1).max_depth(2).into_iter().flatten() {
                                    let p = ent.path();
                                    if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }
                                    let mut in_window = true;
                                    if let Ok(meta) = std::fs::metadata(p) {
                                        if let Ok(modified) = meta.modified() {
                                            let ts = time::OffsetDateTime::from(modified);
                                            if let Some(f) = from_ts { if ts < f { in_window = false; } }
                                            if let Some(t) = to_ts   { if ts > t { in_window = false; } }
                                        }
                                    }
                                    if !in_window { continue; }
                                    let raw = std::fs::read(p).unwrap_or_default();
                                    // stable file name
                                    let dst = dst_root.join(p.file_name().unwrap());
                                    std::fs::write(&dst, &raw).ok();
                                    cline_count += 1;
                                }
                            }
                        }

                        // Inline chain verification for manifest
                        let events_path = stage.join("topics").join("events.ndjson");
                        let (chain_ok, count, first_seq, last_seq) = verify_events_chain_file(&events_path).unwrap_or((false, 0, None, None));

                        // Write bag manifest (v0.2)
                        let manifest = serde_json::json!({
                            "schema": "bag_manifest.v0.2",
                            "version": "0.2",
                            "created_at": time::OffsetDateTime::now_utc().format(&Rfc3339).unwrap(),
                            "format": if out_arg.ends_with(".tar.gz") || out_arg.ends_with(".tgz") { "tgz" } else { "dir" },
                            "counts": {
                                "events": events_count,
                                "term_commands": 0,
                                "files_watch": 0,
                                "cline_tasks": cline_count
                            },
                            "time_range": first_ts.as_ref().zip(last_ts.as_ref()).map(|(s,e)| serde_json::json!({"start": s, "end": e})),
                            "topics": {
                                "events": { "path": "topics/events.ndjson", "schema": "event_v0.3" }
                            },
                            "chain_ok": chain_ok,
                            "run_manifest_path": "run_manifest.json"
                        });
                        std::fs::write(stage.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
                        std::fs::write(stage.join("README.md"), NEXBAG_README.as_bytes())?;

                        // Finalize out
                        let (format, final_out) = if out_arg.ends_with(".tar.gz") || out_arg.ends_with(".tgz") {
                            let parent = std::path::Path::new(&out_arg).parent().unwrap_or(std::path::Path::new("."));
                            std::fs::create_dir_all(parent)?;
                            let f = std::fs::File::create(&out_arg)?;
                            let enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
                            let mut tar = tar::Builder::new(enc);
                            for ent in WalkDir::new(&stage).into_iter().flatten() {
                                let p = ent.path();
                                if p.is_dir() { continue; }
                                let rel = p.strip_prefix(&stage).unwrap();
                                tar.append_path_with_name(p, rel)?;
                            }
                            tar.into_inner()?.finish()?;
                            ("tgz".to_string(), out_arg)
                        } else {
                            let out_path = std::path::Path::new(&out_arg);
                            if out_path != stage {
                                if out_path.exists() {
                                    return Err(anyhow::anyhow!("destination exists: {}", out_arg).into());
                                }
                                std::fs::create_dir_all(out_path.parent().unwrap_or(std::path::Path::new(".")))?;
                                std::fs::rename(&stage, &out_path)?;
                            }
                            ("dir".to_string(), out_arg)
                        };

                        let summary = BagBuildSummary {
                            run_id,
                            format,
                            out: final_out.clone(),
                            counts: BagCounts { events: events_count, term_commands: 0, files_watch: 0, cline_tasks: cline_count },
                            time_range: first_ts.as_ref().zip(last_ts.as_ref()).map(|(s,e)| BagTimeRange { start: s.clone(), end: e.clone() }),
                            chain_ok,
                        };
                        ctx.ok("replay", "bag.build", &summary);
                        Ok(0)
                    }
                    Some(("verify", vsm)) => {
                        let bag = vsm.get_one::<String>("bag").unwrap();
                        let (events_bytes, _from_tgz) = read_bag_topic_bytes(bag, "topics/events.ndjson")?;
                        let (ok, count, first_seq, last_seq, issues, _prev_last) = verify_events_chain_bytes(&events_bytes);
                        let summary = BagVerifyResult {
                            ok,
                            chain_ok: ok,
                            count,
                            first_seq,
                            last_seq,
                            issues,
                        };
                        ctx.ok("replay", "bag.verify", &summary);
                        Ok(0)
                    }
                    Some(("info", ism)) => {
                        let bag = ism.get_one::<String>("bag").unwrap();
                        let (manifest, format) = read_bag_manifest(bag)?;
                        let mut topics = BTreeMap::new();
                        if let Some(t) = manifest.get("topics").cloned() {
                            if let Some(map) = t.as_object() {
                                for (k, v) in map {
                                    topics.insert(k.clone(), v.clone());
                                }
                            }
                        }
                        let versions = {
                            let mut m = BTreeMap::new();
                            m.insert("bag_manifest".to_string(), manifest.get("version").and_then(|x| x.as_str()).unwrap_or("0.2").to_string());
                            if let Some(ev) = manifest.pointer("/topics/events/schema").and_then(|x| x.as_str()) {
                                m.insert("events".to_string(), ev.to_string());
                            }
                            m
                        };
                        let counts = BagCounts {
                            events: manifest.pointer("/counts/events").and_then(|x| x.as_u64()).unwrap_or(0),
                            term_commands: manifest.pointer("/counts/term_commands").and_then(|x| x.as_u64()).unwrap_or(0),
                            files_watch: manifest.pointer("/counts/files_watch").and_then(|x| x.as_u64()).unwrap_or(0),
                            cline_tasks: manifest.pointer("/counts/cline_tasks").and_then(|x| x.as_u64()).unwrap_or(0),
                        };
                        let time_range = manifest.get("time_range").and_then(|tr| {
                            let start = tr.get("start")?.as_str()?.to_string();
                            let end = tr.get("end")?.as_str()?.to_string();
                            Some(BagTimeRange { start, end })
                        });
                        let summary = BagInfoSummary {
                            ok: true,
                            format,
                            topics,
                            versions,
                            counts,
                            time_range,
                        };
                        ctx.ok("replay", "bag.info", &summary);
                        Ok(0)
                    }
                    Some(("cat", csm)) => {
                        let bag = csm.get_one::<String>("bag").unwrap();
                        let topic = csm.get_one::<String>("topic").unwrap().to_string();
                        let max_lines = csm.get_one::<String>("lines").and_then(|s| s.parse::<u64>().ok());
                        let rel = if topic.ends_with(".ndjson") {
                            format!("topics/{}", topic)
                        } else {
                            format!("topics/{}.ndjson", topic)
                        };
                        let (bytes, _from_tgz) = read_bag_topic_bytes(bag, &rel)?;
                        // If bounded, count up to max_lines
                        let mut line_count: u64 = 0;
                        if let Ok(s) = std::str::from_utf8(&bytes) {
                            if let Some(n) = max_lines {
                                for l in s.lines().take(n as usize) { let _ = l; line_count += 1; }
                            } else {
                                line_count = s.lines().count() as u64;
                            }
                        }
                        let summary = CatSummary { ok: true, topic, lines: line_count, bytes: bytes.len() as u64 };
                        ctx.ok("replay", "bag.cat", &summary);
                        Ok(0)
                    }
                    _ => Ok(ctx.err("replay", "bag", "replay.bag.unknown", "unknown bag subcommand", None, 2))
                }
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

// -------- Verification helpers for event chain (v0.3) --------

fn content_hash_for_event(v: &serde_json::Value) -> String {
    let canonical = serde_json::json!({
        "plugin": v.get("plugin"),
        "command": v.get("command"),
        "exit_code": v.get("exit_code"),
        "dur_ms": v.get("dur_ms"),
        "data": v.get("data")
    });
    sha256(&serde_json::to_vec(&canonical).unwrap_or_default())
}

fn verify_events_chain_bytes(bytes: &[u8]) -> (bool, u64, Option<u64>, Option<u64>, Vec<String>, Option<String>) {
    let mut ok = true;
    let mut issues = Vec::new();
    let mut prev = String::new();
    let mut count: u64 = 0;
    let mut first_seq: Option<u64> = None;
    let mut last_seq: Option<u64> = None;

    if let Ok(s) = std::str::from_utf8(bytes) {
        for line in s.lines() {
            if line.trim().is_empty() { continue; }
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(v) => {
                    let want_content = content_hash_for_event(&v);
                    let got_content = v.get("content_hash").and_then(|x| x.as_str()).unwrap_or_default().to_string();
                    if want_content != got_content {
                        ok = false;
                        issues.push(format!("content_hash mismatch at line {}", count+1));
                    }
                    let want_chain = sha256(format!("{}{}", prev, want_content).as_bytes());
                    let got_chain = v.get("chain_hash").and_then(|x| x.as_str()).unwrap_or_default().to_string();
                    if want_chain != got_chain {
                        ok = false;
                        issues.push(format!("chain_hash mismatch at line {}", count+1));
                    }
                    if let Some(p) = v.get("prev").and_then(|x| x.as_str()) {
                        if p != prev {
                            ok = false;
                            issues.push(format!("prev mismatch at line {}", count+1));
                        }
                    }
                    prev = want_chain;
                    if let Some(sq) = v.get("seq").and_then(|x| x.as_u64()) {
                        if first_seq.is_none() { first_seq = Some(sq); }
                        last_seq = Some(sq);
                    }
                    count += 1;
                }
                Err(_) => {
                    ok = false;
                    issues.push(format!("invalid json at line {}", count+1));
                }
            }
        }
    }
    (ok, count, first_seq, last_seq, issues, if prev.is_empty() { None } else { Some(prev) })
}

fn verify_events_chain_file(path: &std::path::Path) -> anyhow::Result<(bool, u64, Option<u64>, Option<u64>)> {
    let bytes = std::fs::read(path).unwrap_or_default();
    let (ok, count, first_seq, last_seq, _issues, _last) = verify_events_chain_bytes(&bytes);
    Ok((ok, count, first_seq, last_seq))
}

// ---------- Bag IO helpers (dir or .tgz) ----------

fn read_bag_manifest(bag: &str) -> anyhow::Result<(serde_json::Value, String)> {
    let p = std::path::Path::new(bag);
    if p.is_dir() {
        let s = std::fs::read_to_string(p.join("manifest.json"))?;
        let v: serde_json::Value = serde_json::from_str(&s)?;
        Ok((v, "dir".to_string()))
    } else {
        // tgz
        let f = std::fs::File::open(p)?;
        let dec = flate2::read::GzDecoder::new(f);
        let mut ar = tar::Archive::new(dec);
        for e in ar.entries()? {
            let mut e = e?;
            let path = e.path()?;
            if path.as_ref() == std::path::Path::new("manifest.json") {
                let mut buf = Vec::new();
                use std::io::Read;
                e.read_to_end(&mut buf)?;
                let v: serde_json::Value = serde_json::from_slice(&buf)?;
                return Ok((v, "tgz".to_string()));
            }
        }
        Err(anyhow::anyhow!("manifest.json not found in bag").into())
    }
}

fn read_bag_topic_bytes(bag: &str, rel: &str) -> anyhow::Result<(Vec<u8>, bool)> {
    let p = std::path::Path::new(bag);
    if p.is_dir() {
        Ok((std::fs::read(p.join(rel)).unwrap_or_default(), false))
    } else {
        let f = std::fs::File::open(p)?;
        let dec = flate2::read::GzDecoder::new(f);
        let mut ar = tar::Archive::new(dec);
        for e in ar.entries()? {
            let mut e = e?;
            let path = e.path()?;
            if path.as_ref() == std::path::Path::new(rel) {
                let mut buf = Vec::new();
                use std::io::Read;
                e.read_to_end(&mut buf)?;
                return Ok((buf, true));
            }
        }
        Ok((Vec::new(), true))
    }
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

const NEXBAG_README: &str = r#"# NEXBag (TracePack v0.2)

This bag captures a single `nexoscli` run in a portable, analyzable format.

## Layout
- `manifest.json` — bag manifest (v0.2)
- `run_manifest.json` — per-run manifest snapshot (v0.2)
- `topics/`
  - `events.ndjson` — newline-delimited events (schema event_v0.3)
  - `term/commands.ndjson` — (reserved)
  - `files/watch.ndjson` — (reserved)
- `cline/tasks/` — imported tasks in time window

## Notes
- Event integrity is verified via hash-chain (content_hash, prev, chain_hash).
- Use `nexoscli replay bag verify --bag <path>` to validate integrity.
"#;
