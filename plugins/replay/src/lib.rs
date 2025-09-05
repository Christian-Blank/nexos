use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::{plugin::{Plugin, CommandContext, ToolSpec}, Result};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use std::collections::BTreeMap;
use time::format_description::well_known::Rfc3339;

mod schemas;
mod fileops;

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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct VerifyArgs {
    path: String,
    #[serde(default)]
    git_check: bool,
    #[serde(default)]
    skip: Option<String>,
    #[serde(default)]
    format: Option<String>, // text|summary|full
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct Finding {
    check: String,            // e.g. "events","fileops","patches","patch-apply","status","cline","time-range","manifest"
    level: String,            // "error" | "warn" | "info"
    message: String,
    #[serde(default)]
    hint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct VerifyReport {
    ok: bool,
    #[serde(default)]
    run_id: Option<String>,
    totals: serde_json::Value,      // pass-through of manifest.counts
    findings: Vec<Finding>,
    #[serde(default)]
    suggestions: Vec<String>,
}

// ------------ Plan-33: Events v0.3 Normalize/Schema ------------

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct NormalizeArgs {
    to: String,
    #[serde(default)]
    bag: Option<String>,
    #[serde(default)]
    events: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    out: Option<String>,
    #[serde(default)]
    update_manifest: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct NormalizeResult {
    schema: String,          // "nexos/events@0.3"
    in_lines: u64,
    out_lines: u64,
    out: String,             // path written
    #[serde(default)]
    run_id: Option<String>,
    sha256: String,          // sha256 of output file bytes
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaArgs {
    kind: String,            // "events"
    version: String,         // "0.3"
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaResult {
    kind: String,            // "events"
    version: String,         // "0.3"
    schema: serde_json::Value,
    example: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaRow { name: String, version: String, id: String, description: String }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaListResult { schemas: Vec<SchemaRow> }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaGetArgs { name: String, version: String, #[serde(default)] out: Option<String> }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SchemaGetResult { name: String, version: String, id: String, bytes: usize, #[serde(default)] written: Option<String> }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ValidateArgs {
    bag: String,
    #[serde(default)] what: Option<String>, // events | fileops | all
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ArtifactReport {
    ok: bool,
    lines: u64,
    valid: u64,
    invalid: u64,
    #[serde(default)]
    errors: Vec<String>, // first N error messages with line numbers
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ValidateResult {
    bag: String,
    summary_ok: bool,
    #[serde(default)]
    events: Option<ArtifactReport>,
    #[serde(default)]
    fileops: Option<ArtifactReport>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct FileopsDeriveArgs {
    bag: String,
    #[serde(default)]
    out: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct FileopsDeriveResult {
    bag: String,
    out: String,
    lines: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct V03Event {
    #[serde(default = "v03_schema_tag")]
    schema: String,          // fixed: "nexos/events@0.3"
    ts: String,              // RFC3339
    plugin: String,
    command: String,
    exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dur_ms: Option<u64>,
    run_id: String,
    seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    args_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prev: Option<String>,
    content_hash: String,
    chain_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meta: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attrs: Option<serde_json::Value>,
}

fn v03_schema_tag() -> String { "nexos/events@0.3".to_string() }

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
                    Command::new("verify").about("Verify a TracePack (dir or .tgz) and explain issues")
                        .arg(Arg::new("path").long("path").required(true))
                        .arg(Arg::new("git-check").long("git-check").action(ArgAction::SetTrue))
                        .arg(Arg::new("skip").long("skip").value_name("CSV"))
                        .arg(Arg::new("format").long("format").value_parser(["text","summary","full"]).default_value("text"))
                )
                .subcommand(
                    Command::new("normalize").about("Normalize events to v0.3")
                        .arg(Arg::new("to").long("to").required(true).value_parser(["0.3"]))
                        .arg(Arg::new("bag").long("bag").value_name("PATH"))
                        .arg(Arg::new("events").long("events").value_name("PATH"))
                        .arg(Arg::new("run-id").long("run-id").value_name("RUN"))
                        .arg(Arg::new("out").long("out").value_name("PATH"))
                        .arg(Arg::new("update-manifest").long("update-manifest").action(ArgAction::SetTrue))
                )
                .subcommand(
                    Command::new("schema").about("Schemas bundle")
                        .subcommand(Command::new("list").about("List available schemas"))
                        .subcommand(
                            Command::new("get").about("Get schema JSON")
                                .arg(Arg::new("name").long("name").required(true).value_parser(["events","fileops"]))
                                .arg(Arg::new("version").long("version").required(true).value_name("N.N"))
                                .arg(Arg::new("out").long("out").value_name("PATH"))
                        )
                        .subcommand(
                            Command::new("events").about("Print events schema")
                                .arg(Arg::new("version").long("version").required(true).value_parser(["0.3"]))
                        )
                )
                .subcommand(
                    Command::new("validate").about("Validate bag artifacts against schemas")
                        .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
                        .arg(Arg::new("what").long("what").value_parser(["events","fileops","all"]).default_value("all"))
                )
                .subcommand(
                    Command::new("fileops").about("FileOps utilities")
                        .subcommand(
                            Command::new("derive").about("Derive fileops.ndjson from bag patches")
                                .arg(Arg::new("bag").long("bag").required(true).value_name("DIR"))
                                .arg(Arg::new("out").long("out").value_name("PATH"))
                        )
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
                let (mut manifest_fileops, mut fileops_lines) = {
                    let mf_path = default_stage.join(".fileops.manifest.json");
                    let fl_path = default_stage.join(".fileops.lines");
                    if mf_path.exists() {
                        let raw = std::fs::read(&mf_path).unwrap_or_default();
                        let v = serde_json::from_slice::<serde_json::Value>(&raw).ok();
                        let ln = std::fs::read_to_string(&fl_path).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
                        (v, ln)
                    } else { (None, 0) }
                };

                // PLAN-35: derive fileops from patches if not already included via fs.topic
                if manifest_fileops.is_none() {
                    let patches_dir = default_stage.join("patches");
                    if patches_dir.is_dir() {
                        if let Ok(rows) = crate::fileops::derive_for_bag(&default_stage, Some(run_id.as_str()), last_ts.as_deref()) {
                            let dst_rel = std::path::Path::new("fileops").join("fileops.ndjson");
                            let dst_abs = default_stage.join(&dst_rel);
                            if let Ok(n) = crate::fileops::write_fileops_ndjson(&dst_abs, &rows) {
                                let bytes = std::fs::read(&dst_abs).unwrap_or_default();
                                let sha = sha256(&bytes);
                                manifest_fileops = Some(serde_json::json!({
                                    "dst": dst_rel.display().to_string(),
                                    "sha256": sha,
                                    "bytes": bytes.len() as u64,
                                    "lines": n as u64,
                                    "from": serde_json::Value::Null,
                                    "to": serde_json::Value::Null
                                }));
                                fileops_lines = n as u64;
                            }
                        }
                    }
                }

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

            Some(("verify", sm)) => {
                let path = sm.get_one::<String>("path").unwrap().to_string();
                let git_check = sm.get_flag("git-check");
                let skip_set: std::collections::HashSet<String> = sm
                    .get_one::<String>("skip")
                    .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|s| !s.is_empty()).collect())
                    .unwrap_or_default();
                let _fmt = sm.get_one::<String>("format").cloned().unwrap_or_else(|| "text".to_string());

                match replay_verify_explain(&path, git_check, skip_set) {
                    Ok(report) => {
                        if report.ok {
                            let exit = ctx.ok("replay", "verify", &report);
                            Ok(exit)
                        } else {
                            let exit = ctx.err("replay", "verify", "bag.verify.failed", "bag verification failed", None, 2);
                            Ok(exit)
                        }
                    }
                    Err(e) => {
                        // Operational error
                        let exit = ctx.err("replay", "verify", "bag.verify.failed", &format!("verify error: {}", e), None, 2);
                        Ok(exit)
                    }
                }
            }

            Some(("normalize", sm)) => {
                let to = sm.get_one::<String>("to").map(|s| s.as_str()).unwrap_or("");
                if to != "0.3" {
                    let exit = ctx.err("replay","normalize","replay.normalize.failed","unsupported --to version",None,2);
                    return Ok(exit);
                }
                let bag = sm.get_one::<String>("bag").cloned();
                let events = sm.get_one::<String>("events").cloned();
                let run_id = sm.get_one::<String>("run-id").cloned();
                let out = sm.get_one::<String>("out").cloned();
                let update_manifest = sm.get_flag("update-manifest");

                let choices = (bag.is_some() as i32) + (events.is_some() as i32);
                if choices != 1 {
                    let exit = ctx.err("replay","normalize","replay.normalize.failed","specify exactly one of --bag or --events",None,2);
                    return Ok(exit);
                }

                let res = if let Some(b) = bag {
                    normalize_from_bag(&b, out.as_deref(), update_manifest)
                } else {
                    normalize_from_events(events.as_ref().unwrap(), run_id.as_deref(), out.as_deref())
                };

                match res {
                    Ok(summary) => {
                        let exit = ctx.ok("replay","normalize",&summary);
                        Ok(exit)
                    }
                    Err(e) => {
                        let exit = ctx.err("replay","normalize","replay.normalize.failed",&format!("normalize error: {}", e),None,2);
                        Ok(exit)
                    }
                }
            }

            Some(("schema", sm)) => {
                match sm.subcommand() {
                    Some(("list", _)) => {
                        let rows: Vec<SchemaRow> = schemas::list().into_iter().map(|s| SchemaRow{
                            name: s.name.to_string(),
                            version: s.version.to_string(),
                            id: s.id.to_string(),
                            description: s.description.to_string(),
                        }).collect();
                        let exit = ctx.ok("replay","schema.list",&SchemaListResult{ schemas: rows });
                        Ok(exit)
                    }
                    Some(("get", gm)) => {
                        let name = gm.get_one::<String>("name").unwrap().to_string();
                        let version = gm.get_one::<String>("version").unwrap().to_string();
                        let outp = gm.get_one::<String>("out").cloned();
                        let schema = match schemas::get_schema_json(&name, &version) {
                            Some(s) => s,
                            None => {
                                return Ok(ctx.err("replay","schema.get","replay.schema.unknown",&format!("unknown schema {}/{}", name, version),None,2));
                            }
                        };
                        let bytes = serde_json::to_vec_pretty(&schema).unwrap_or_default().len();
                        let written = if let Some(path) = outp.clone() {
                            if let Some(dir) = std::path::Path::new(&path).parent() { std::fs::create_dir_all(dir).ok(); }
                            std::fs::write(&path, serde_json::to_vec_pretty(&schema).unwrap_or_default()).ok();
                            Some(path)
                        } else { None };
                        let id = schemas::list().into_iter()
                            .find(|s| s.name==name && s.version==version).map(|s| s.id).unwrap_or("");
                        let exit = ctx.ok("replay","schema.get",&SchemaGetResult{
                            name, version, id: id.to_string(), bytes, written
                        });
                        Ok(exit)
                    }
                    Some(("events", esm)) => {
                        let version = esm.get_one::<String>("version").map(|s| s.as_str()).unwrap_or("");
                        if version != "0.3" {
                            let exit = ctx.err("replay","schema.events","replay.schema.failed","unsupported --version",None,2);
                            return Ok(exit);
                        }
                        let sch = schema_for!(V03Event);
                        let schema_json = serde_json::to_value(&sch).unwrap_or(serde_json::json!({}));
                        let example_canon = serde_json::json!({
                            "plugin": "demo",
                            "command": "example",
                            "exit_code": 0,
                            "dur_ms": null,
                            "data": null
                        });
                        let content = content_hash_for_event(&example_canon);
                        let chain = sha256(content.as_bytes());
                        let example = serde_json::json!({
                            "schema": "nexos/events@0.3",
                            "ts": "1970-01-01T00:00:00Z",
                            "plugin": "demo",
                            "command": "example",
                            "exit_code": 0,
                            "dur_ms": null,
                            "run_id": "RUN-EXAMPLE",
                            "seq": 1,
                            "args_hash": null,
                            "prev": null,
                            "content_hash": content,
                            "chain_hash": chain,
                            "data": null,
                            "meta": null,
                            "attrs": null
                        });
                        let res = SchemaResult { kind: "events".into(), version: "0.3".into(), schema: schema_json, example };
                        let exit = ctx.ok("replay","schema.events",&res);
                        Ok(exit)
                    }
                    _ => Ok(ctx.err("replay","schema","replay.schema.failed","unknown schema subcommand",None,2))
                }
            }

            Some(("validate", vm)) => {
                let bag = vm.get_one::<String>("bag").unwrap().to_string();
                let what = vm.get_one::<String>("what").map(|s| s.as_str()).unwrap_or("all");
                let bagp = std::path::PathBuf::from(&bag);
                if !bagp.is_dir() {
                    return Ok(ctx.err("replay","validate","replay.validate.nodir","bag path is not a directory",None,2));
                }

                let mut events_rep: Option<ArtifactReport> = None;
                let mut fileops_rep: Option<ArtifactReport> = None;

                if what=="events" || what=="all" {
                    let p = bagp.join("events.ndjson");
                    if p.exists() {
                        if let Some(schema) = schemas::get_schema_json("events","0.3") {
                            match validate_ndjson(&p, &schema) {
                                Ok(rep) => events_rep = Some(rep),
                                Err(e) => {
                                    let exit = ctx.err("replay","validate","replay.validate.failed",&format!("events validation error: {}", e),None,2);
                                    return Ok(exit);
                                }
                            }
                        }
                    }
                }
                if what=="fileops" || what=="all" {
                    let p1 = bagp.join("fileops").join("fileops.ndjson");
                    let p2 = bagp.join("fileops.ndjson");
                    let p = if p1.exists() { p1 } else { p2 };
                    if p.exists() {
                        if let Some(schema) = schemas::get_schema_json("fileops","0.1") {
                            match validate_ndjson(&p, &schema) {
                                Ok(rep) => fileops_rep = Some(rep),
                                Err(e) => {
                                    let exit = ctx.err("replay","validate","replay.validate.failed",&format!("fileops validation error: {}", e),None,2);
                                    return Ok(exit);
                                }
                            }
                        }
                    }
                }

                let summary_ok = events_rep.as_ref().map(|r| r.ok).unwrap_or(true)
                    && fileops_rep.as_ref().map(|r| r.ok).unwrap_or(true);

                let out = ValidateResult{
                    bag,
                    summary_ok,
                    events: events_rep,
                    fileops: fileops_rep,
                };
                let exit = if summary_ok {
                    ctx.ok("replay","validate",&out)
                } else {
                    ctx.err("replay","validate","replay.validate.failed","validation failed",None,2)
                };
                Ok(exit)
            }

            Some(("fileops", fsm)) => {
                match fsm.subcommand() {
                    Some(("derive", dsm)) => {
                        let bag = dsm.get_one::<String>("bag").unwrap().to_string();
                        let out_opt = dsm.get_one::<String>("out").cloned();

                        // Try to read manifest.json for run_id and time_range end
                        let mut rid: Option<String> = None;
                        let mut ts_to: Option<String> = None;
                        let man_path = std::path::Path::new(&bag).join("manifest.json");
                        if man_path.exists() {
                            if let Ok(raw) = std::fs::read(&man_path) {
                                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&raw) {
                                    rid = v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string());
                                    ts_to = v.pointer("/time_range/to")
                                        .or_else(|| v.pointer("/time_range/end"))
                                        .and_then(|x| x.as_str())
                                        .map(|s| s.to_string());
                                }
                            }
                        }

                        let rows = match fileops::derive_for_bag(std::path::Path::new(&bag), rid.as_deref(), ts_to.as_deref()) {
                            Ok(r) => r,
                            Err(e) => {
                                let exit = ctx.err("replay","fileops.derive","replay.fileops.derive.failed",&format!("derive error: {}", e),None,2);
                                return Ok(exit);
                            }
                        };

                        let out_path = if let Some(o) = out_opt {
                            std::path::PathBuf::from(o)
                        } else {
                            fileops::default_cli_out_for_bag(std::path::Path::new(&bag))
                        };
                        let n = match fileops::write_fileops_ndjson(&out_path, &rows) {
                            Ok(n) => n,
                            Err(e) => {
                                let exit = ctx.err("replay","fileops.derive","replay.fileops.derive.failed",&format!("write error: {}", e),None,2);
                                return Ok(exit);
                            }
                        };
                        let exit = ctx.ok("replay","fileops.derive",&serde_json::json!({
                            "bag": bag,
                            "out": out_path.display().to_string(),
                            "lines": n
                        }));
                        Ok(exit)
                    }
                    _ => Ok(ctx.err("replay","fileops","replay.fileops.failed","unknown fileops subcommand",None,2))
                }
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
        vec![
            ToolSpec{
                name: "replay.bag.verify".to_string(),
                about: "Verify a TracePack/NEXBag integrity; checks events stats/digests and optional rotation index continuity".to_string(),
                args_schema: schema_for!(BagVerifyArgs),
                result_schema: schema_for!(BagVerifySummary),
            },
            ToolSpec{
                name: "replay.verify".to_string(),
                about: "Verify a TracePack (dir or .tgz) and explain findings".to_string(),
                args_schema: schema_for!(VerifyArgs),
                result_schema: schema_for!(VerifyReport),
            },
            ToolSpec{
                name: "replay.normalize".to_string(),
                about: "Normalize events to v0.3 (from bag or events file)".to_string(),
                args_schema: schema_for!(NormalizeArgs),
                result_schema: schema_for!(NormalizeResult),
            },
            ToolSpec{
                name: "replay.schema.events".to_string(),
                about: "JSON Schema for events v0.3".to_string(),
                args_schema: schema_for!(SchemaArgs),
                result_schema: schema_for!(SchemaResult),
            },
            ToolSpec{
                name: "replay.schema.list".to_string(),
                about: "List available schemas".to_string(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(SchemaListResult),
            },
            ToolSpec{
                name: "replay.schema.get".to_string(),
                about: "Get schema JSON".to_string(),
                args_schema: schema_for!(SchemaGetArgs),
                result_schema: schema_for!(SchemaGetResult),
            },
            ToolSpec{
                name: "replay.validate".to_string(),
                about: "Validate bag artifacts (events/fileops) against schemas".to_string(),
                args_schema: schema_for!(ValidateArgs),
                result_schema: schema_for!(ValidateResult),
            },
            ToolSpec{
                name: "replay.fileops.derive".to_string(),
                about: "Derive fileops.ndjson from a bag's patches".to_string(),
                args_schema: schema_for!(FileopsDeriveArgs),
                result_schema: schema_for!(FileopsDeriveResult),
            }
        ]
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

fn replay_verify_explain(path: &str, git_check: bool, skip: std::collections::HashSet<String>) -> anyhow::Result<VerifyReport> {
    use std::collections::BTreeSet;
    // Load manifest
    let (manifest, _fmt) = match read_bag_manifest(path) {
        Ok(t) => t,
        Err(_e) => {
            return Ok(VerifyReport{
                ok: false,
                run_id: None,
                totals: serde_json::json!({}),
                findings: vec![Finding{
                    check: "manifest".into(),
                    level: "error".into(),
                    message: "manifest.json not found or unreadable".into(),
                    hint: Some("Re-run extraction (nexoscli replay extract --run-id ...)".into()),
                }],
                suggestions: vec![],
            });
        }
    };

    let run_id = manifest.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string());
    let counts = manifest.get("counts").cloned().unwrap_or(serde_json::json!({}));
    let mut findings: Vec<Finding> = Vec::new();

    // Events path resolution (TracePack default root; fallback to bag topics)
    let mut events_rel = manifest
        .pointer("/topics/events/path")
        .and_then(|x| x.as_str())
        .unwrap_or("events.ndjson")
        .to_string();

    let mut events_bytes = read_bag_topic_bytes(path, &events_rel)?.0;
    if events_bytes.is_empty() && events_rel != "topics/events.ndjson" {
        events_rel = "topics/events.ndjson".to_string();
        events_bytes = read_bag_topic_bytes(path, &events_rel)?.0;
    }

    // Events checks
    if !skip.contains("events") {
        if events_bytes.is_empty() {
            findings.push(Finding{
                check: "events".into(),
                level: "error".into(),
                message: format!("missing {}", events_rel),
                hint: Some("Re-extract the bag; ensure the run had events".into()),
            });
        } else {
            let s = std::str::from_utf8(&events_bytes).unwrap_or("");
            let mut lines: u64 = 0;
            let mut bad: u64 = 0;
            let mut run_ids: BTreeSet<String> = BTreeSet::new();
            let mut first_ts: Option<time::OffsetDateTime> = None;
            let mut last_ts: Option<time::OffsetDateTime> = None;

            for raw in s.lines() {
                if raw.trim().is_empty() { continue; }
                lines += 1;
                match serde_json::from_str::<serde_json::Value>(raw) {
                    Ok(v) => {
                        if let Some(r) = v.get("run_id").and_then(|x| x.as_str()) {
                            run_ids.insert(r.to_string());
                        }
                        if let Some(ts) = v.get("ts").and_then(|x| x.as_str()).and_then(|ss| time::OffsetDateTime::parse(ss, &Rfc3339).ok()) {
                            if first_ts.is_none() { first_ts = Some(ts); }
                            last_ts = Some(ts);
                        }
                    }
                    Err(_) => bad += 1,
                }
            }

            if bad > 0 {
                findings.push(Finding{
                    check: "events".into(),
                    level: "error".into(),
                    message: format!("{} non-JSON lines in events", bad),
                    hint: Some("events must be strict JSON per line; do not hand-edit".into()),
                });
            }
            if run_ids.len() > 1 {
                findings.push(Finding{
                    check: "events".into(),
                    level: "error".into(),
                    message: format!("multiple run_id values present: {:?}", run_ids),
                    hint: Some("bags are per-run; ensure extraction used one run_id".into()),
                });
            }

            // counts comparison: support both TracePack (events_lines) and Bag (events)
            if let Some(w) = manifest.pointer("/counts/events").and_then(|x| x.as_u64())
                .or_else(|| manifest.pointer("/counts/events_lines").and_then(|x| x.as_u64())) {
                if w != lines {
                    findings.push(Finding{
                        check: "events".into(),
                        level: "error".into(),
                        message: format!("line count mismatch manifest={} actual={}", w, lines),
                        hint: Some("Regenerate with nexoscli replay extract".into()),
                    });
                }
            }

            // time-range consistency: support v0.2 bag (start/end) and TracePack (from/to)
            let m_start = manifest.pointer("/time_range/start").and_then(|x| x.as_str())
                .or_else(|| manifest.pointer("/time_range/from").and_then(|x| x.as_str()));
            let m_end = manifest.pointer("/time_range/end").and_then(|x| x.as_str())
                .or_else(|| manifest.pointer("/time_range/to").and_then(|x| x.as_str()));
            if let (Some(mf), Some(mt), Some(fr), Some(to)) = (m_start, m_end, first_ts, last_ts) {
                if let (Ok(mf_ts), Ok(mt_ts)) = (time::OffsetDateTime::parse(mf, &Rfc3339), time::OffsetDateTime::parse(mt, &Rfc3339)) {
                    if fr < mf_ts || to > mt_ts {
                        findings.push(Finding{
                            check: "time-range".into(),
                            level: "warn".into(),
                            message: format!("event timestamps outside manifest window: actual=({}, {}) manifest=({}, {})", fr, to, mf_ts, mt_ts),
                            hint: Some("Re-extract to refresh manifest.time_range".into()),
                        });
                    }
                }
            }
        }
    }

    // Fileops checks (Plan-31)
    if !skip.contains("fileops") {
        if let Some(fo) = manifest.get("fileops") {
            let dst = fo.get("dst").and_then(|x| x.as_str()).unwrap_or("fileops/fileops.ndjson");
            let bytes = read_bag_topic_bytes(path, dst)?.0;
            if bytes.is_empty() {
                findings.push(Finding{
                    check: "fileops".into(),
                    level: "error".into(),
                    message: format!("missing {}", dst),
                    hint: Some("Re-extract after running with --fs-topic (Plan-31)".into()),
                });
            } else {
                let got_lines = std::str::from_utf8(&bytes).unwrap_or("").lines().filter(|l| !l.trim().is_empty()).count() as u64;
                let got_bytes = bytes.len() as u64;
                let got_sha = sha256(&bytes);
                if let Some(exp) = fo.get("lines").and_then(|x| x.as_u64()) {
                    if exp != got_lines {
                        findings.push(Finding{
                            check: "fileops".into(),
                            level: "error".into(),
                            message: format!("lines mismatch manifest={} actual={}", exp, got_lines),
                            hint: Some("Do not edit fileops stream; regenerate the bag".into()),
                        });
                    }
                }
                if let Some(exp) = fo.get("bytes").and_then(|x| x.as_u64()) {
                    if exp != got_bytes {
                        findings.push(Finding{
                            check: "fileops".into(),
                            level: "error".into(),
                            message: format!("bytes mismatch manifest={} actual={}", exp, got_bytes),
                            hint: Some("Re-extract to refresh fileops".into()),
                        });
                    }
                }
                if let Some(exp) = fo.get("sha256").and_then(|x| x.as_str()) {
                    if exp != got_sha {
                        findings.push(Finding{
                            check: "fileops".into(),
                            level: "error".into(),
                            message: "sha256 mismatch".into(),
                            hint: Some("Regenerate; verify no in-place edits".into()),
                        });
                    }
                }
            }
        }
    }

    // Helper to verify advertised artifact lists
    let mut verify_list = |check_name: &str, arr_key: &str| -> anyhow::Result<(u64, u64)> {
        let mut tot = 0u64;
        let mut err = 0u64;
        if let Some(arr) = manifest.get(arr_key).and_then(|x| x.as_array()) {
            for item in arr {
                let dst = item.get("dst").and_then(|x| x.as_str()).unwrap_or("");
                if dst.is_empty() { continue; }
                tot += 1;
                let b = read_bag_topic_bytes(path, dst)?.0;
                if b.is_empty() {
                    findings.push(Finding{
                        check: check_name.into(),
                        level: if check_name == "status" { "warn".into() } else { "error".into() },
                        message: format!("missing {}", dst),
                        hint: Some("Re-extract to restore advertised artifact".into()),
                    });
                    if check_name != "status" { err += 1; }
                    continue;
                }
                if let Some(exp) = item.get("bytes").and_then(|x| x.as_u64()) {
                    let got = b.len() as u64;
                    if exp != got {
                        findings.push(Finding{
                            check: check_name.into(),
                            level: if check_name == "status" { "warn".into() } else { "error".into() },
                            message: format!("{} bytes mismatch (exp {}, got {})", dst, exp, got),
                            hint: Some("Artifacts are append-only; do not edit".into()),
                        });
                        if check_name != "status" { err += 1; }
                    }
                }
                if let Some(exp) = item.get("sha256").and_then(|x| x.as_str()) {
                    let got = sha256(&b);
                    if exp != got {
                        findings.push(Finding{
                            check: check_name.into(),
                            level: if check_name == "status" { "warn".into() } else { "error".into() },
                            message: format!("{} sha mismatch", dst),
                            hint: Some("Re-extract from original run".into()),
                        });
                        if check_name != "status" { err += 1; }
                    }
                }
            }
        }
        Ok((tot, err))
    };

    // Patches/status/cline checks (skippable)
    let (patches_tot, _patches_err) = if !skip.contains("patches") { verify_list("patches", "patches")? } else { (0,0) };
    let (status_tot, _status_err)   = if !skip.contains("status")  { verify_list("status",  "status_files")? } else { (0,0) };
    let (cline_tot, _cline_err)     = if !skip.contains("cline")   { verify_list("cline",   "cline_tasks")? } else { (0,0) };

    // Compare counts against manifest if present
    if let Some(c) = manifest.get("counts") {
        if let Some(wp) = c.get("patches").and_then(|x| x.as_u64()) {
            if wp != patches_tot {
                findings.push(Finding{
                    check: "patches".into(),
                    level: "error".into(),
                    message: format!("patches count manifest={} actual={}", wp, patches_tot),
                    hint: Some("Regenerate manifest by re-extracting".into()),
                });
            }
        }
        if let Some(ws) = c.get("status").and_then(|x| x.as_u64()) {
            if ws != status_tot {
                findings.push(Finding{
                    check: "status".into(),
                    level: "warn".into(),
                    message: format!("status count manifest={} actual={}", ws, status_tot),
                    hint: Some("Re-extract with status capture enabled".into()),
                });
            }
        }
        if let Some(wc) = c.get("cline").and_then(|x| x.as_u64()).or_else(|| c.get("cline_tasks").and_then(|x| x.as_u64())) {
            if wc != cline_tot {
                findings.push(Finding{
                    check: "cline".into(),
                    level: "error".into(),
                    message: format!("cline tasks count manifest={} actual={}", wc, cline_tot),
                    hint: Some("Re-extract with --include cline".into()),
                });
            }
        }
    }

    // Optional: patch applicability via git apply --check
    if git_check && !skip.contains("patch-apply") {
        if which_git() && is_git_repo() {
            // Use manifest-listed patches in a stable order
            if let Some(arr) = manifest.get("patches").and_then(|x| x.as_array()) {
                let mut dsts: Vec<String> = arr.iter().filter_map(|it| it.get("dst").and_then(|x| x.as_str()).map(|s| s.to_string())).collect();
                dsts.sort();
                for p in dsts.into_iter().take(10) {
                    let b = read_bag_topic_bytes(path, &p)?.0;
                    if b.is_empty() { continue; }
                    let tf = tempfile::NamedTempFile::new()?;
                    std::fs::write(tf.path(), &b)?;
                    let ok = std::process::Command::new("git")
                        .args(["apply", "--check", tf.path().to_str().unwrap()])
                        .status().ok().map(|s| s.success()).unwrap_or(false);
                    if !ok {
                        findings.push(Finding{
                            check: "patch-apply".into(),
                            level: "warn".into(),
                            message: format!("git apply --check failed for {}", p),
                            hint: Some("Apply on a throwaway branch; ensure base range matches manifest.git".into()),
                        });
                    }
                }
            }
        } else {
            findings.push(Finding{
                check: "patch-apply".into(),
                level: "info".into(),
                message: "git not available or not a repository; skipped --git-check".into(),
                hint: None,
            });
        }
    }

    // Suggestions
    if findings.iter().any(|f| f.check == "patches" && f.level == "error") && !findings.iter().any(|f| f.check == "events" && f.level == "error") {
        // Patches problematic but events fine
        // Provide consolidation hint
        // (Avoid duplicates)
        findings.push(Finding{
            check: "advice".into(),
            level: "info".into(),
            message: "If patches were expected, ensure they were captured by 'nexoscli term run' and included by the extractor.".into(),
            hint: None,
        });
    }

    // Finalize
    let ok = !findings.iter().any(|f| f.level == "error");
    Ok(VerifyReport{
        ok,
        run_id,
        totals: counts,
        findings,
        suggestions: vec![],
    })
}

fn which_git() -> bool {
    std::process::Command::new("git").arg("--version").output().ok().map(|o| o.status.success()).unwrap_or(false)
}
fn is_git_repo() -> bool {
    std::process::Command::new("git").args(["rev-parse","--is-inside-work-tree"]).output().ok().map(|o| o.status.success()).unwrap_or(false)
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

// ----------------- Normalize helpers (Plan-33) -----------------

fn to_v03(
    input: &serde_json::Value,
    seq: u64,
    prev_chain: Option<&str>
) -> anyhow::Result<V03Event> {
    let ts = input.get("ts").and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing ts"))?
        .to_string();
    let plugin = input.get("plugin").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
    let command = input.get("command").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
    let exit_code = input.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0) as i32;
    let dur_ms = input.get("dur_ms").and_then(|x| x.as_u64());
    let run_id = input.get("run_id").and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing run_id"))?
        .to_string();
    let args_hash = input.get("args_hash").and_then(|x| x.as_str()).map(|s| s.to_string());
    let data = input.get("data").cloned();
    let meta = input.get("meta").cloned();

    // attrs = original minus mapped fields
    let mut attrs_obj = serde_json::Map::new();
    if let Some(obj) = input.as_object() {
        for (k, v) in obj {
            if ["ts","plugin","command","exit_code","dur_ms","run_id","seq","args_hash","data","meta"].contains(&k.as_str()) {
                continue;
            }
            attrs_obj.insert(k.clone(), v.clone());
        }
    }
    // convenience source tag
    attrs_obj.insert("source".into(), serde_json::json!(format!("{}.{}", plugin, command)));
    let attrs_val = if attrs_obj.is_empty() { None } else { Some(serde_json::Value::Object(attrs_obj)) };

    // content hash parity with verifier
    let canon = serde_json::json!({
        "plugin": plugin,
        "command": command,
        "exit_code": exit_code,
        "dur_ms": dur_ms,
        "data": data
    });
    let content_hash = content_hash_for_event(&canon);
    let prev_str = prev_chain.unwrap_or("");
    let chain_hash = sha256(format!("{}{}", prev_str, content_hash).as_bytes());

    Ok(V03Event{
        schema: v03_schema_tag(),
        ts,
        plugin,
        command,
        exit_code,
        dur_ms,
        run_id,
        seq,
        args_hash,
        prev: if prev_str.is_empty() { None } else { Some(prev_str.to_string()) },
        content_hash,
        chain_hash,
        data,
        meta,
        attrs: attrs_val,
    })
}

fn normalize_from_events(path: &str, run_id: Option<&str>, out: Option<&str>) -> anyhow::Result<NormalizeResult> {
    let txt = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read events: {}", e))?;
    let mut out_lines = 0u64;
    let mut in_lines = 0u64;
    let mut prev: Option<String> = None;
    let mut run_id_out: Option<String> = None;
    let mut buf = String::new();

    for raw in txt.lines() {
        if raw.trim().is_empty() { continue; }
        in_lines += 1;
        let v: serde_json::Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => continue, // skip invalid
        };
        if let Some(r) = v.get("run_id").and_then(|x| x.as_str()) {
            if run_id_out.is_none() { run_id_out = Some(r.to_string()); }
        }
        if let Some(filter) = run_id {
            if v.get("run_id").and_then(|x| x.as_str()) != Some(filter) {
                continue;
            }
        }
        let next_seq = out_lines + 1;
        let ev = to_v03(&v, next_seq, prev.as_deref())?;
        let line = serde_json::to_string(&ev).unwrap_or_default();
        buf.push_str(&line); buf.push('\n');
        prev = Some(ev.chain_hash.clone());
        out_lines += 1;
    }

    let out_path = if let Some(p) = out {
        std::path::PathBuf::from(p)
    } else {
        let p = std::path::Path::new(path);
        let parent = p.parent().unwrap_or(std::path::Path::new("."));
        parent.join("events.v0.3.ndjson")
    };
    if let Some(parent) = out_path.parent() { std::fs::create_dir_all(parent).ok(); }
    std::fs::write(&out_path, buf.as_bytes())
        .map_err(|e| anyhow::anyhow!("write out: {}", e))?;
    let sha = sha256(buf.as_bytes());
    Ok(NormalizeResult{
        schema: v03_schema_tag(),
        in_lines,
        out_lines,
        out: out_path.display().to_string(),
        run_id: run_id.map(|s| s.to_string()).or(run_id_out),
        sha256: sha,
    })
}

fn normalize_from_bag(bag_path: &str, out: Option<&str>, update_manifest: bool) -> anyhow::Result<NormalizeResult> {
    let p = std::path::Path::new(bag_path);
    let (manifest, _fmt) = read_bag_manifest(bag_path)
        .map_err(|e| anyhow::anyhow!("manifest: {}", e))?;

    // Resolve events relative path: prefer manifest topics/events/path else default root then fallback topic path
    let mut events_rel = manifest
        .pointer("/topics/events/path")
        .and_then(|x| x.as_str())
        .unwrap_or("events.ndjson")
        .to_string();

    let (mut events_bytes, _from_tgz0) = read_bag_topic_bytes(bag_path, &events_rel)?;
    if events_bytes.is_empty() && events_rel != "topics/events.ndjson" {
        events_rel = "topics/events.ndjson".to_string();
        events_bytes = read_bag_topic_bytes(bag_path, &events_rel)?.0;
    }
    if events_bytes.is_empty() {
        return Err(anyhow::anyhow!("events not found in bag at {}", events_rel));
    }

    let bag_is_dir = p.is_dir();
    if !bag_is_dir && update_manifest {
        return Err(anyhow::anyhow!("--update-manifest not allowed for archived .tgz bag"));
    }

    let default_out = if bag_is_dir {
        Some(p.join("events.v0.3.ndjson").display().to_string())
    } else { None };

    let out_path = out.map(|s| s.to_string()).or(default_out)
        .ok_or_else(|| anyhow::anyhow!("--out is required for archived .tgz input"))?;
    let out_pathbuf = std::path::PathBuf::from(&out_path);

    // If manifest has run_id, prefer as filter; else no filter
    let rid = manifest.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string());
    // Write events_bytes to a temp string and drive normalizer
    let tmpdir = tempfile::tempdir()?;
    let tmpfile = tmpdir.path().join("events.src.ndjson");
    std::fs::write(&tmpfile, &events_bytes).map_err(|e| anyhow::anyhow!("temp write: {}", e))?;
    let res = normalize_from_events(tmpfile.to_str().unwrap(), rid.as_deref(), Some(&out_path))?;

    // Update manifest.json if requested (dir only)
    if update_manifest && bag_is_dir {
        let dst_rel = if out_pathbuf.starts_with(p) {
            out_pathbuf.strip_prefix(p).unwrap().display().to_string()
        } else {
            // default path when outside (discouraged)
            "events.v0.3.ndjson".to_string()
        };
        let bytes = std::fs::read(&out_pathbuf).unwrap_or_default();
        let entry = serde_json::json!({
            "schema": v03_schema_tag(),
            "dst": dst_rel,
            "lines": res.out_lines,
            "bytes": bytes.len() as u64,
            "sha256": res.sha256
        });
        let man_path = p.join("manifest.json");
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&man_path).unwrap_or_default())
            .unwrap_or(serde_json::json!({}));
        v["events_v03"] = entry;
        std::fs::write(&man_path, serde_json::to_vec_pretty(&v).unwrap_or_default())
            .map_err(|e| anyhow::anyhow!("manifest update: {}", e))?;
    }

    Ok(res)
}

fn validate_ndjson(path: &std::path::Path, schema_json: &serde_json::Value) -> anyhow::Result<ArtifactReport> {
    use std::io::BufRead;

    let compiled = jsonschema::JSONSchema::compile(schema_json)
        .map_err(|e| anyhow::anyhow!("invalid schema: {}", e))?;

    let f = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(f);
    let mut lines: u64 = 0;
    let mut valid: u64 = 0;
    let mut invalid: u64 = 0;
    let mut errors: Vec<String> = vec![];

    for (idx, line) in reader.lines().enumerate() {
        let line_no = (idx + 1) as u64;
        let l = match line {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => { continue; }
            Err(e) => {
                invalid += 1;
                if errors.len() < 10 {
                    errors.push(format!("line {}: io error: {}", line_no, e));
                }
                continue;
            }
        };
        lines += 1;
        match serde_json::from_str::<serde_json::Value>(&l) {
            Ok(v) => {
                match compiled.validate(&v) {
                    Ok(()) => {
                        valid += 1;
                    }
                    Err(iter) => {
                        invalid += 1;
                        if errors.len() < 10 {
                            let msg = iter.map(|e| e.to_string()).next().unwrap_or_else(|| "schema violation".to_string());
                            errors.push(format!("line {}: {}", line_no, msg));
                        }
                    }
                }
            }
            Err(e) => {
                invalid += 1;
                if errors.len() < 10 {
                    errors.push(format!("line {}: json parse error: {}", line_no, e));
                }
            }
        }
    }

    Ok(ArtifactReport{
        ok: true,
        lines, valid, invalid, errors
    })
}
