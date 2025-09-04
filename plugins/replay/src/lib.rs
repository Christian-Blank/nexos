use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::{plugin::{Plugin, CommandContext, ToolSpec}, Result};
use schemars::{JsonSchema, schema_for};
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagVerifyArgs {
    bag: String,                 // dir or .tgz/.tar.gz
    #[serde(default)]
    index: Option<String>,       // optional path to events.index.json (Plan-29)
    #[serde(default)]
    strict: bool,                // strict time_range equality, stricter digest checks
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagVerifyIssue {
    kind: String,                // "missing", "mismatch", "parse", "warning"
    #[serde(default)]
    path: Option<String>,
    message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct BagVerifySummary {
    ok: bool,
    bag_dir: String,
    #[serde(default)]
    manifest_schema: Option<String>,
    counts: serde_json::Value,
    events: serde_json::Value,        // computed stats (lines/bytes/sha256/from/to)
    #[serde(default)]
    index_check: Option<serde_json::Value>, // result when --index provided or auto-found
    issues: Vec<BagVerifyIssue>,

    // Back-compat for existing tests expecting these fields at top-level
    #[serde(default)]
    chain_ok: Option<bool>,
    #[serde(default)]
    count: Option<u64>,
    #[serde(default)]
    first_seq: Option<u64>,
    #[serde(default)]
    last_seq: Option<u64>,
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
    Command::new("verify").about("Verify a TracePack/NEXBag integrity")
        .arg(Arg::new("bag").long("bag").required(true).help("Path to bag dir or .tgz/.tar.gz"))
        .arg(Arg::new("index").long("index").required(false).value_name("PATH").help("Optional path to .nexoscli/log/events.index.json"))
        .arg(Arg::new("strict").long("strict").action(ArgAction::SetTrue).help("Strict mode: exact time_range equality; require digests when present"))
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
                    // track fs topic event linkage
                    let mut fs_topic_path: Option<String> = None;

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
                                        if fs_topic_path.is_none()
                                            && v.get("plugin").and_then(|x| x.as_str()) == Some("term")
                                            && v.get("command").and_then(|x| x.as_str()) == Some("fs.topic") {
                                            if let Some(fp) = v.get("data").and_then(|d| d.get("file")).and_then(|x| x.as_str()) {
                                                fs_topic_path = Some(fp.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    std::fs::write(default_stage.join("events.ndjson"), out.as_bytes())?;

                    // If not already found, detect fs.topic linkage from staged events
                    if fs_topic_path.is_none() {
                        let evp = default_stage.join("events.ndjson");
                        if let Ok(evtxt) = std::fs::read_to_string(&evp) {
                            for l in evtxt.lines() {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                                    if v.get("run_id").and_then(|x| x.as_str()) == Some(run_id.as_str())
                                        && v.get("plugin").and_then(|x| x.as_str())==Some("term")
                                        && v.get("command").and_then(|x| x.as_str())==Some("fs.topic") {
                                        if let Some(fp) = v.get("data").and_then(|d| d.get("file")).and_then(|x| x.as_str()) {
                                            fs_topic_path = Some(fp.to_string());
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Try to include fileops into the TracePack (copy + manifest fields)
                    let mut fileops_lines: u64 = 0;
                    let mut manifest_fileops: Option<serde_json::Value> = None;
                    if let Some(info) = include_fileops_for_extract(fs_topic_path.as_deref(), first_ts.as_deref(), last_ts.as_deref(), &default_stage) {
                        fileops_lines = info.get("lines").and_then(|x| x.as_u64()).unwrap_or(0);
                        manifest_fileops = Some(info);
                    }

                    // Store for later use when writing the manifest
                    // We stash into a file so we can retrieve after cline/status/patches handling
                    if let Some(mf) = &manifest_fileops {
                        std::fs::write(default_stage.join(".fileops.manifest.json"), serde_json::to_vec(mf).unwrap_or_default()).ok();
                        std::fs::write(default_stage.join(".fileops.lines"), fileops_lines.to_string()).ok();
                    }
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
                // Reload fileops info (if any) recorded during events processing
                let (manifest_fileops, fileops_lines) = {
                    let mf_path = default_stage.join(".fileops.manifest.json");
                    let fl_path = default_stage.join(".fileops.lines");
                    if mf_path.exists() {
                        let raw = std::fs::read(&mf_path).unwrap_or_default();
                        let v = serde_json::from_slice::<serde_json::Value>(&raw).ok();
                        let ln = std::fs::read_to_string(&fl_path).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
                        (v, ln)
                    } else { (None, 0) }
                };

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
                        "cline": counts.cline,
                        "fileops_lines": fileops_lines
                    },
                    "git": { "from": plan.from, "to": plan.to },
                    "plugins": plugin_counts,
                    "commands": cmd_counts,
                    "patches": manifest_patches,
                    "status_files": manifest_status,
                    "cline_tasks": manifest_cline,
                    "fileops": manifest_fileops
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
                        // Args
                        let args = BagVerifyArgs {
                            bag: vsm.get_one::<String>("bag").unwrap().to_string(),
                            index: vsm.get_one::<String>("index").cloned(),
                            strict: vsm.get_flag("strict"),
                        };

                        // Read manifest (dir or tgz)
                        let (manifest, _fmt) = match read_bag_manifest(&args.bag) {
                            Ok(t) => t,
                            Err(_e) => {
                                // manifest missing → verification failed
                                let exit = ctx.err("replay","bag.verify","replay.bag.verify.failed","manifest.json not found or unreadable",None,2);
                                return Ok(exit);
                            }
                        };

                        // Resolve events path from manifest (v0.2) or default
                        let events_rel = manifest
                            .pointer("/topics/events/path")
                            .and_then(|x| x.as_str())
                            .unwrap_or("topics/events.ndjson")
                            .to_string();

                        let (events_bytes, _from_tgz) = read_bag_topic_bytes(&args.bag, &events_rel)?;
                        // Chain check (v0.3 events): used for compatibility fields
                        let (chain_ok, count, first_seq, last_seq, _chain_issues, _prev_last) = verify_events_chain_bytes(&events_bytes);

                        // Compute deterministic stats over events bytes (newline-joined hashing)
                        let estats = events_stats_from_bytes(&events_bytes);
                        // Build issues
                        let mut issues: Vec<BagVerifyIssue> = Vec::new();

                        // Compare counts (support both v0.2 counts.events and older events_lines)
                        let want_events = manifest.pointer("/counts/events").and_then(|x| x.as_u64())
                            .or_else(|| manifest.pointer("/counts/events_lines").and_then(|x| x.as_u64()));
                        if let Some(w) = want_events {
                            if estats.get("lines").and_then(|x| x.as_u64()) != Some(w) {
                                issues.push(BagVerifyIssue{
                                    kind: "mismatch".into(),
                                    path: Some(events_rel.clone()),
                                    message: format!("events count manifest={} actual={}", w, estats.get("lines").and_then(|x| x.as_u64()).unwrap_or(0)),
                                });
                            }
                        } else {
                            // If manifest declares zero topics/events but events file has lines, warn
                            if estats.get("lines").and_then(|x| x.as_u64()).unwrap_or(0) > 0 {
                                issues.push(BagVerifyIssue{
                                    kind: "warning".into(),
                                    path: Some(events_rel.clone()),
                                    message: "manifest missing counts.events but events present".into(),
                                });
                            }
                        }

                        // Compare time_range: v0.2 uses {start,end}, older may use {from,to}
                        let m_start = manifest.pointer("/time_range/start").and_then(|x| x.as_str())
                            .or_else(|| manifest.pointer("/time_range/from").and_then(|x| x.as_str()));
                        let m_end   = manifest.pointer("/time_range/end").and_then(|x| x.as_str())
                            .or_else(|| manifest.pointer("/time_range/to").and_then(|x| x.as_str()));
                        let a_from = estats.get("from_ts").and_then(|x| x.as_str());
                        let a_to   = estats.get("to_ts").and_then(|x| x.as_str());
                        if let (Some(mf), Some(mt), Some(af), Some(at)) = (m_start, m_end, a_from, a_to) {
                            if args.strict {
                                if mf != af || mt != at {
                                    issues.push(BagVerifyIssue{
                                        kind:"mismatch".into(), path: Some(events_rel.clone()),
                                        message: format!("time_range strict mismatch manifest=({},{}) actual=({},{})", mf, mt, af, at)
                                    });
                                }
                            } else {
                                // relaxed containment
                                if af < mf || at > mt {
                                    issues.push(BagVerifyIssue{
                                        kind:"warning".into(), path: Some(events_rel.clone()),
                                        message: format!("time_range containment mismatch manifest=({},{}) actual=({},{})", mf, mt, af, at)
                                    });
                                }
                            }
                        }

                        // Optional: manifest sha256 for events (forward-compatible)
                        if let Some(msha) = manifest.get("sha256_events").and_then(|x| x.as_str()) {
                            if estats.get("sha256_file").and_then(|x| x.as_str()) != Some(msha) {
                                issues.push(BagVerifyIssue{
                                    kind:"mismatch".into(), path: Some(events_rel.clone()),
                                    message: format!("sha256_events manifest={} actual={}", msha, estats.get("sha256_file").and_then(|x| x.as_str()).unwrap_or("?"))
                                });
                            }
                        }

                        // Optional: artifacts if arrays present (patches/status_files/cline_tasks)
                        let mut verify_list = |key: &str| -> (u64, u64) {
                            let mut err = 0u64; let mut tot = 0u64;
                            if let Some(arr) = manifest.get(key).and_then(|x| x.as_array()) {
                                for item in arr {
                                    tot += 1;
                                    let dst = item.get("dst").and_then(|x| x.as_str()).unwrap_or("");
                                    let sha = item.get("sha256").and_then(|x| x.as_str()).unwrap_or("");
                                    if dst.is_empty() || sha.is_empty() {
                                        issues.push(BagVerifyIssue{ kind:"parse".into(), path: None, message: format!("{} entry missing dst/sha256", key) });
                                        err += 1; continue;
                                    }
                                    let (bytes, _tgz) = read_bag_topic_bytes(&args.bag, dst).unwrap_or((Vec::new(), false));
                                    if bytes.is_empty() {
                                        issues.push(BagVerifyIssue{ kind:"missing".into(), path: Some(dst.to_string()), message: format!("{} missing", key) });
                                        err += 1; continue;
                                    }
                                    let actual = sha256(&bytes);
                                    if actual != sha {
                                        issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: Some(dst.to_string()),
                                            message: format!("{} sha mismatch manifest={} actual={}", key, sha, actual) });
                                        err += 1;
                                    }
                                }
                            }
                            (tot, err)
                        };
                        let (patches_tot, _patches_err) = verify_list("patches");
                        let (status_tot, _status_err)   = verify_list("status_files");
                        let (cline_tot, _cline_err)     = verify_list("cline_tasks");

                        // Compare counts for artifacts if present
                        if let Some(c) = manifest.get("counts") {
                            if let Some(wp) = c.get("patches").and_then(|x| x.as_u64()) {
                                if wp != patches_tot {
                                    issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: None, message: format!("patches count manifest={} actual={}", wp, patches_tot) });
                                }
                            }
                            if let Some(ws) = c.get("status").and_then(|x| x.as_u64()) {
                                if ws != status_tot {
                                    issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: None, message: format!("status count manifest={} actual={}", ws, status_tot) });
                                }
                            }
                            // Support both "cline" and "cline_tasks"
                            if let Some(wc) = c.get("cline").and_then(|x| x.as_u64()).or_else(|| c.get("cline_tasks").and_then(|x| x.as_u64())) {
                                if wc != cline_tot {
                                    issues.push(BagVerifyIssue{ kind:"mismatch".into(), path: None, message: format!("cline tasks count manifest={} actual={}", wc, cline_tot) });
                                }
                            }
                        }

                        // Optional: index continuity
                        let auto_index = {
                            let p = std::env::current_dir().ok().and_then(|d| {
                                let ip = d.join(".nexoscli").join("log").join("events.index.json");
                                if ip.exists() { Some(ip.display().to_string()) } else { None }
                            });
                            p
                        };
                        let mut index_obj: Option<serde_json::Value> = None;
                        if let Some(ip) = args.index.clone().or(auto_index) {
                            let path = std::path::PathBuf::from(&ip);
                            if path.exists() {
                                match verify_index_head(&path) {
                                    Ok(v) => index_obj = Some(v),
                                    Err(e) => issues.push(BagVerifyIssue{ kind:"parse".into(), path: Some(ip), message: format!("index verify: {}", e) }),
                                }
                            } else {
                                issues.push(BagVerifyIssue{ kind:"warning".into(), path: Some(ip), message:"index path not found".into() });
                            }
                        }

                        // Final summary
                        let ok = issues.iter().all(|i| i.kind != "mismatch" && i.kind != "missing" && i.kind != "parse");
                        let summary = BagVerifySummary{
                            ok,
                            bag_dir: args.bag.clone(),
                            manifest_schema: manifest.get("schema").and_then(|x| x.as_str()).map(|s| s.to_string()),
                            counts: manifest.get("counts").cloned().unwrap_or(serde_json::json!({})),
                            events: estats,
                            index_check: index_obj,
                            issues,
                            chain_ok: Some(chain_ok),
                            count: Some(count),
                            first_seq,
                            last_seq,
                        };

                        if ok {
                            let exit = ctx.ok("replay", "bag.verify", &summary);
                            Ok(exit)
                        } else {
                            let exit = ctx.err("replay","bag.verify","replay.bag.verify.failed","bag verification failed",None,2);
                            Ok(exit)
                        }
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

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec{
            name: "replay.bag.verify".to_string(),
            about: "Verify a TracePack/NEXBag integrity; checks events stats/digests and optional rotation index continuity".to_string(),
            args_schema: schema_for!(BagVerifyArgs),
            result_schema: schema_for!(BagVerifySummary),
        }]
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

// -------- Bag verify helpers --------

fn events_stats_from_bytes(bytes: &[u8]) -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let mut lines: u64 = 0;
    let mut h = Sha256::new();
    let mut first = true;
    let mut from_ts: Option<String> = None;
    let mut to_ts: Option<String> = None;

    if let Ok(s) = std::str::from_utf8(bytes) {
        for raw in s.lines() {
            if raw.trim().is_empty() { continue; }
            if !first { h.update(&[0x0a]); }
            h.update(raw.as_bytes());
            first = false;

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
                if from_ts.is_none() {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { from_ts = Some(ts.to_string()); }
                }
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) { to_ts = Some(ts.to_string()); }
            }
            lines += 1;
        }
    }
    let sha = format!("sha256:{:x}", h.finalize());
    serde_json::json!({
        "lines": lines,
        "bytes": bytes.len() as u64,
        "sha256_file": sha,
        "from_ts": from_ts,
        "to_ts": to_ts
    })
}

fn verify_index_head(index_path: &std::path::Path) -> anyhow::Result<serde_json::Value> {
    // Minimal IndexFile matching dev rotation format
    #[derive(Debug, serde::Deserialize)]
    struct IndexSeg {
        file: String,
        #[serde(default)] from_ts: Option<String>,
        #[serde(default)] to_ts: Option<String>,
        lines: u64,
        bytes: u64,
        sha256_file: String,
        #[serde(default)] prev_head: Option<String>,
        head: String,
    }
    #[derive(Debug, serde::Deserialize)]
    struct IndexFile {
        schema: String,
        segments: Vec<IndexSeg>,
        #[serde(default)] head: Option<String>,
        #[serde(default)] updated_at: Option<String>,
    }

    let txt = std::fs::read_to_string(index_path)?;
    let idx: IndexFile = serde_json::from_str(&txt)?;
    // base root is repo root: index_path/.nexoscli/log/events.index.json -> root = parent of parent
    let base_root = index_path.parent().and_then(|p| p.parent()).unwrap_or(std::path::Path::new("."));
    let mut head: Option<String> = None;
    let mut issues: Vec<String> = vec![];
    let mut checked = 0u64;

    for seg in &idx.segments {
        let p = base_root.join(&seg.file);
        if !p.exists() {
            issues.push(format!("missing segment {}", seg.file));
            continue;
        }
        // recompute sha for the segment using same newline-joined hashing
        let s = compute_event_file_stats_path(&p)?;
        if s.sha256_file != seg.sha256_file {
            issues.push(format!("segment sha mismatch {} manifest={} actual={}", seg.file, seg.sha256_file, s.sha256_file));
        }
        // recompute head
        let actual_head = chain_next(head.as_deref(), &seg.sha256_file);
        if actual_head != seg.head {
            issues.push(format!("segment head mismatch {} manifest={} actual={}", seg.file, seg.head, actual_head));
        }
        head = Some(actual_head);
        checked += 1;
    }

    let idx_head_ok = idx.head.as_deref() == head.as_deref();
    if !idx_head_ok {
        issues.push(format!("index head mismatch manifest={:?} actual={:?}", idx.head, head));
    }

    Ok(serde_json::json!({
        "path": index_path.display().to_string(),
        "segments_checked": checked,
        "computed_head": head,
        "head_ok": idx_head_ok,
        "issues": issues
    }))
}

#[derive(Debug)]
struct ComputePathStats {
    lines: u64,
    bytes: u64,
    sha256_file: String,
}
fn compute_event_file_stats_path(ev: &std::path::Path) -> anyhow::Result<ComputePathStats> {
    use sha2::{Digest, Sha256};
    use std::io::BufRead;
    let mut lines: u64 = 0;
    let mut bytes: u64 = 0;
    let mut h = Sha256::new();

    if ev.exists() {
        let f = std::fs::File::open(ev)?;
        bytes = f.metadata().map(|m| m.len()).unwrap_or(0);
        let rdr = std::io::BufReader::new(f);
        let mut first = true;
        for line in rdr.lines() {
            let raw = line?;
            if raw.trim().is_empty() { continue; }
            if !first { h.update(&[0x0a]); }
            h.update(raw.as_bytes());
            first = false;
            lines += 1;
        }
    }
    let sha = format!("sha256:{:x}", h.finalize());
    Ok(ComputePathStats{ lines, bytes, sha256_file: sha })
}

fn chain_next(prev: Option<&str>, item: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    if let Some(p) = prev {
        if !p.is_empty() { h.update(p.as_bytes()); h.update(&[0x0a]); }
    }
    h.update(item.as_bytes());
    format!("sha256:{:x}", h.finalize())
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

fn include_fileops_for_extract(
    explicit_path: Option<&str>,
    first_ts: Option<&str>,
    last_ts: Option<&str>,
    stage: &std::path::Path
) -> Option<serde_json::Value> {
    use sha2::{Digest, Sha256};
    use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
    let dst_rel = std::path::Path::new("fileops").join("fileops.ndjson");
    let dst_abs = stage.join(&dst_rel);

    // Helper to compute stats and write
    let compute = |bytes: &[u8]| -> (u64, u64, String, Option<String>, Option<String>) {
        let mut h = Sha256::new();
        h.update(bytes);
        let sha = format!("sha256:{:x}", h.finalize());
        let s = std::str::from_utf8(bytes).unwrap_or("");
        let mut from: Option<String> = None;
        let mut to: Option<String> = None;
        let mut lines: u64 = 0;
        for l in s.lines() {
            if l.trim().is_empty() { continue; }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l) {
                if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                    if from.is_none() { from = Some(ts.to_string()); }
                    to = Some(ts.to_string());
                }
            }
            lines += 1;
        }
        (lines, bytes.len() as u64, sha, from, to)
    };

    // Prefer explicit path from fs.topic event
    if let Some(fp) = explicit_path {
        let src = std::path::Path::new(fp);
        if src.exists() {
            if let Some(parent) = dst_abs.parent() { let _ = std::fs::create_dir_all(parent); }
            if std::fs::copy(src, &dst_abs).is_ok() {
                let raw = std::fs::read(&dst_abs).unwrap_or_default();
                let (lines, bytes, sha, from, to) = compute(&raw);
                return Some(serde_json::json!({
                    "dst": dst_rel.display().to_string(),
                    "sha256": sha,
                    "bytes": bytes,
                    "lines": lines,
                    "from": from,
                    "to": to
                }));
            }
        }
    }

    // Fallback: scan .nexoscli/fileops for window overlap
    let dir = std::path::Path::new(".nexoscli").join("fileops");
    let rf = first_ts.and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t - Duration::seconds(5));
    let rt = last_ts.and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()).map(|t| t + Duration::seconds(5));

    if dir.exists() {
        for ent in WalkDir::new(&dir).min_depth(1).max_depth(1).into_iter().flatten() {
            let p = ent.path();
            if p.is_dir() || p.extension().and_then(|x| x.to_str()) != Some("ndjson") { continue; }
            let txt = std::fs::read_to_string(p).unwrap_or_default();
            let mut first: Option<OffsetDateTime> = None;
            let mut last: Option<OffsetDateTime> = None;
            for line in txt.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok()) {
                        if first.is_none() { first = Some(ts); }
                        last = Some(ts);
                    }
                }
            }
            let overlaps = match (rf, rt, first, last) {
                (Some(f), Some(t), Some(a), Some(b)) => a <= t && b >= f,
                _ => false
            };
            if overlaps {
                if let Some(parent) = dst_abs.parent() { let _ = std::fs::create_dir_all(parent); }
                if std::fs::write(&dst_abs, txt.as_bytes()).is_ok() {
                    let (lines, bytes, sha, from, to) = compute(txt.as_bytes());
                    return Some(serde_json::json!({
                        "dst": dst_rel.display().to_string(),
                        "sha256": sha,
                        "bytes": bytes,
                        "lines": lines,
                        "from": from,
                        "to": to
                    }));
                }
            }
        }
    }
    None
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
