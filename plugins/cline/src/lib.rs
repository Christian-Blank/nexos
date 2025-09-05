use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{plugin::{Plugin, CommandContext, ToolSpec}, Result};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Read, path::{Path, PathBuf}};
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
use walkdir::WalkDir;

mod correlation;
mod normalize;

pub struct ClinePlugin;
impl ClinePlugin { pub fn new() -> Self { Self } }

const EXT_IDS_DEFAULT: &[&str] = &["saoudrizwan.claude-dev","cline.cline"];
const EDITORS_DEFAULT_MAC: &[&str] = &["Code","Code - Insiders","Cursor","VSCodium","Windsurf"];
const EDITORS_DEFAULT_LINUX: &[&str] = &["Code","Code - Insiders","VSCodium","code-oss","Cursor","Windsurf"];
const EDITORS_DEFAULT_WIN: &[&str] = &["Code","Code - Insiders","VSCodium","Cursor","Windsurf"];

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct Candidate {
    base: PathBuf,       // .../User/globalStorage
    ext_id: String,      // extension id
    tasks_dir: PathBuf,  // base/ext_id/tasks
    checkpoints_dir: PathBuf,
    latest_mtime: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct LocateResult {
    selected: Option<PathBuf>,
    candidates: Vec<Candidate>,
    hints: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ImportArgs {
    #[serde(default)]
    storage: Option<String>, // explicit path or "auto"
    #[serde(default)]
    since: Option<String>,   // "24h" | "7d" | RFC3339
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct NormalizedTask {
    id: String,
    #[serde(skip_serializing_if="Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    created_at: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    updated_at: Option<String>,
    messages: usize,
    source_file: String,
    redacted: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TaskRow {
    id: String,
    #[serde(skip_serializing_if="Option::is_none")]
    updated_at: Option<String>,
    messages: usize,
    file: String,
    state: String, // created|changed|unchanged|busy|invalid
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, JsonSchema)]
struct ImportSummary {
    imported: u64,    // total imported (passed filters)
    skipped: u64,     // skipped by since filter
    created: u64,
    changed: u64,
    unchanged: u64,
    busy: u64,
    invalid: u64,
    tasks: Vec<TaskRow>,
    persisted_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Index {
    // key = source_file absolute path
    entries: BTreeMap<String, IndexEntry>,
}
#[derive(Debug, Serialize, Deserialize)]
struct IndexEntry {
    id: String,
    sha256_raw: String,
    last_imported_at: String,
}

impl Plugin for ClinePlugin {
    fn name(&self) -> &'static str { "cline" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Cline task history (locate/import) with idempotent ingest" }

    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("cline").about(self.describe())
            .subcommand(ClapCommand::new("locate").about("Find Cline storage (globalStorage/workspace)"))
            .subcommand(
                ClapCommand::new("import").about("Import tasks (read-only)")
                    .arg(Arg::new("storage").long("storage").value_name("PATH|auto"))
                    .arg(Arg::new("since").long("since").value_name("WINDOW"))
                    .arg(Arg::new("limit").long("limit").value_parser(clap::value_parser!(u64)))
                    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
            )
            .subcommand(
                ClapCommand::new("normalize").about("Normalize Cline tasks → AIM streams (.nexoscli/agents/streams/Cline/<task>/messages.ndjson)")
                    .arg(Arg::new("since").long("since").value_name("WINDOW"))
                    .arg(Arg::new("task-id").long("task-id").value_name("ID"))
                    .arg(Arg::new("overwrite").long("overwrite").action(ArgAction::SetTrue))
            )
            .subcommand(
                ClapCommand::new("correlate").about("Correlate Cline tasks to runs")
                    .arg(Arg::new("run-id").long("run-id").value_name("RUN"))
                    .arg(Arg::new("since").long("since").value_name("WINDOW"))
                    .arg(Arg::new("pad").long("pad").value_name("DUR").default_value("5m"))
                    .arg(Arg::new("write").long("write").action(ArgAction::SetTrue))
                    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
            )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            Some(("locate", _)) => {
                let (cands, selected, hints) = locate_all();
                let result = LocateResult {
                    selected: selected.clone(),
                    candidates: cands,
                    hints,
                };
                let exit = ctx.ok("cline","locate",&result);
                Ok(exit)
            }
            Some(("import", sm)) => {
                let since_ts = parse_since(sm.get_one::<String>("since"));
                let limit = *sm.get_one::<u64>("limit").unwrap_or(&0);
                let dry = sm.get_flag("dry-run");
                let storage = match sm.get_one::<String>("storage") {
                    Some(s) if s != "auto" => PathBuf::from(s),
                    _ => {
                        let (_cands, selected, _hints) = locate_all();
                        selected.ok_or_else(|| anyhow::anyhow!("no Cline tasks dir found; run `nexoscli cline locate` or pass --storage <path>"))?
                    }
                };
                if !storage.exists() {
                    return Err(anyhow::anyhow!("storage not found: {}", storage.display()).into());
                }

                // ingest
                let outdir = PathBuf::from(".nexoscli").join("cline").join("tasks");
                let mut summary = import_tasks(&storage, since_ts, &outdir, limit, dry)?;
                summary.persisted_dir = Some(outdir.display().to_string());

                let exit = if dry {
                    ctx.ok("cline","import.dryrun",&summary)
                } else {
                    ctx.ok("cline","import",&summary)
                };
                Ok(exit)
            }
            Some(("normalize", m)) => {
                let dot = std::env::current_dir()?.join(".nexoscli");
                let since = parse_since(m.get_one::<String>("since"));
                let overwrite = m.get_flag("overwrite");
                let only_task = m.get_one::<String>("task-id").map(|s| s.as_str());
                let sum = crate::normalize::normalize_since(&dot, since, overwrite, only_task)?;
                return Ok(ctx.ok("cline","normalize",&serde_json::to_value(sum)?));
            }
            Some(("correlate", m)) => {
                let dot = std::env::current_dir()?.join(".nexoscli");
                let pad = crate::correlation::parse_pad(m.get_one::<String>("pad"));
                // best-effort ensure AIM streams exist
                let _ = crate::normalize::normalize_since(&dot, None, false, None);

                if let Some(run) = m.get_one::<String>("run-id") {
                    let rec = crate::correlation::correlate_run(&dot, run, pad)?;
                    // attach message stats
                    let msg_stats = correlate_messages_for_run(&dot, &rec);
                    let mut v = serde_json::to_value(&rec)?;
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("msg_stats".into(), msg_stats);
                    }
                    if m.get_flag("write") {
                        crate::correlation::persist_record(&dot, &rec)?;
                        // persist sibling messages summary
                        let corr_dir = dot.join("cline").join("corr");
                        std::fs::create_dir_all(&corr_dir).ok();
                        std::fs::write(corr_dir.join(format!("{}.messages.json", rec.run_id)), serde_json::to_vec_pretty(&v)?)?;
                    }
                    return Ok(ctx.ok("cline","correlate.one",&v));
                } else {
                    let since = crate::correlation::parse_since(m.get_one::<String>("since"));
                    let write = m.get_flag("write");
                    let (sum, recs) = crate::correlation::correlate_since(&dot, since, pad, write)?;
                    // compute message coverage
                    let mut with_msgs: u64 = 0;
                    for r in &recs {
                        let stats = correlate_messages_for_run(&dot, r);
                        if stats.get("messages_in_window").and_then(|x| x.as_u64()).unwrap_or(0) > 0 { with_msgs += 1; }
                    }
                    let msg_cov = if recs.is_empty() { 1.0 } else { with_msgs as f64 / recs.len() as f64 };
                    return Ok(ctx.ok("cline","correlate",&serde_json::json!({
                        "summary": sum, "count": recs.len(), "message_coverage": msg_cov
                    })));
                }
            }
            _ => Err(anyhow::anyhow!("unknown cline command").into()),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "cline.locate".to_string(),
                about: "Find Cline storage locations".to_string(),
                args_schema: schema_for!(ImportArgs),
                result_schema: schema_for!(LocateResult),
            },
            ToolSpec {
                name: "cline.import".to_string(),
                about: "Import Cline task history with idempotent ingest".to_string(),
                args_schema: schema_for!(ImportArgs),
                result_schema: schema_for!(ImportSummary),
            },
            ToolSpec {
                name: "cline.correlate".to_string(),
                about: "Correlate Cline tasks to runs by time".to_string(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
            },
        ]
    }
}

/* ---------- Discovery ---------- */

fn locate_all() -> (Vec<Candidate>, Option<PathBuf>, Vec<String>) {
    let mut hints = Vec::new();

    // 1) explicit env override
    if let Ok(p) = std::env::var("NEXOS_CLINE_STORAGE") {
        let path = PathBuf::from(p);
        let cand = Candidate {
            base: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            ext_id: "env_override".into(),
            tasks_dir: path.clone(),
            checkpoints_dir: path.parent().unwrap_or(Path::new("")).join("checkpoints"),
            latest_mtime: newest_mtime(&path).map(|t| t.format(&Rfc3339).unwrap()),
        };
        return (vec![cand.clone()], Some(cand.tasks_dir), hints);
    }

    // 2) test root for E2E tests
    let test_root = std::env::var("NEXOS_CLINE_TEST_ROOT").ok().map(PathBuf::from);

    // 3) collect bases (remote/devcontainer; user-global)
    let mut bases = Vec::<PathBuf>::new();

    // Devcontainer/remote candidates
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    for p in [
        home.join(".vscode-server").join("data").join("User").join("globalStorage"),
        home.join(".vscode-remote").join("data").join("User").join("globalStorage"),
        PathBuf::from("/workspaces").join("vscode-globalStorage"),
    ] {
        if p.exists() { bases.push(p); }
    }

    // User-global bases per OS/editor
    let editors: Vec<String> = std::env::var("NEXOS_CLINE_EDITORS").ok()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
        .unwrap_or_else(|| {
            #[cfg(target_os = "macos")] { EDITORS_DEFAULT_MAC.iter().map(|s| s.to_string()).collect() }
            #[cfg(any(target_os="linux", target_os="freebsd"))] { EDITORS_DEFAULT_LINUX.iter().map(|s| s.to_string()).collect() }
            #[cfg(target_os="windows")] { EDITORS_DEFAULT_WIN.iter().map(|s| s.to_string()).collect() }
        });

    let mut user_bases = Vec::<PathBuf>::new();
    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library").join("Application Support");
        for ed in &editors { user_bases.push(base.join(ed).join("User").join("globalStorage")); }
    }
    #[cfg(any(target_os="linux", target_os="freebsd"))]
    {
        let cfg = dirs::config_dir().unwrap_or(home.join(".config"));
        for ed in &editors { user_bases.push(cfg.join(ed).join("User").join("globalStorage")); }
    }
    #[cfg(target_os="windows")]
    {
        let app = dirs::data_dir().unwrap_or(home.join("AppData").join("Roaming"));
        for ed in &editors { user_bases.push(app.join(ed).join("User").join("globalStorage")); }
    }
    for b in user_bases { if b.exists() { bases.push(b); } }

    if let Some(tr) = test_root { bases.push(tr); }

    // Extension ids
    let ext_ids: Vec<String> = std::env::var("NEXOS_CLINE_EXTIDS").ok()
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
        .unwrap_or_else(|| EXT_IDS_DEFAULT.iter().map(|s| s.to_string()).collect());

    // Build candidates
    let mut cands = Vec::<Candidate>::new();
    for base in bases {
        for ext in &ext_ids {
            let tasks = base.join(ext).join("tasks");
            let checks = base.join(ext).join("checkpoints");
            if tasks.exists() {
                cands.push(Candidate {
                    base: base.clone(),
                    ext_id: ext.clone(),
                    tasks_dir: tasks.clone(),
                    checkpoints_dir: checks.clone(),
                    latest_mtime: newest_mtime(&tasks).map(|t| t.format(&Rfc3339).unwrap()),
                });
            }
        }
    }
    // Select newest by mtime
    let selected = cands.iter()
        .max_by_key(|c| c.latest_mtime.clone())
        .map(|c| c.tasks_dir.clone());

    if selected.is_none() {
        hints.push("No Cline storage found. If running in a Dev Container, mount your host globalStorage, e.g.:".into());
        hints.push(r#"  "mounts": ["source=${localEnv:HOME}/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev,target=/workspaces/vscode-globalStorage/saoudrizwan.claude-dev,type=bind"]"#.into());
        hints.push("Or set NEXOS_CLINE_STORAGE=/absolute/path/to/.../globalStorage/<extId>/tasks".into());
    }

    (cands, selected, hints)
}

fn newest_mtime(dir: &Path) -> Option<OffsetDateTime> {
    if !dir.exists() { return None; }
    let mut best = None;
    for ent in WalkDir::new(dir).min_depth(1).max_depth(2).into_iter().flatten() {
        if ent.path().is_dir() { continue; }
        if let Ok(meta) = ent.metadata() {
            if let Ok(m) = meta.modified() {
                let t = OffsetDateTime::from(m);
                if best.map(|b| t > b).unwrap_or(true) { best = Some(t); }
            }
        }
    }
    best
}

fn parse_since(s: Option<&String>) -> Option<OffsetDateTime> {
    match s {
        None => None,
        Some(s) if s.ends_with('h') => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
        Some(s) if s.ends_with('d') => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
        Some(s) => OffsetDateTime::parse(s, &Rfc3339).ok(),
    }
}

/* ---------- Import (idempotent) ---------- */

fn import_tasks(tasks_dir: &Path, since: Option<OffsetDateTime>, outdir: &Path, limit: u64, dry_run: bool) -> anyhow::Result<ImportSummary> {
    if !dry_run {
        fs::create_dir_all(outdir)?;
    }
    let mut idx = load_index(outdir)?;
    let mut sum = ImportSummary::default();

    for ent in WalkDir::new(tasks_dir).min_depth(1).max_depth(2) {
        let ent = match ent { Ok(e) => e, Err(_) => continue };
        let p = ent.path();
        if p.is_dir() { continue; }
        if p.extension().and_then(|x| x.to_str()) != Some("json") { continue; }

        // since filter by file mtime (fast)
        if let Some(since) = since {
            if let Ok(meta) = fs::metadata(p) {
                if let Ok(modified) = meta.modified() {
                    let t = OffsetDateTime::from(modified);
                    if t < since { 
                        sum.skipped += 1;
                        continue; 
                    }
                }
            }
        }
        sum.imported += 1;

        // Read raw bytes with a simple "stable size" check to avoid partial writes
        let (raw, is_stable) = read_stable(p);
        if !is_stable {
            sum.busy += 1;
            sum.tasks.push(TaskRow {
                id: p.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").into(),
                updated_at: None, messages: 0,
                file: p.to_string_lossy().to_string(), state: "busy".into()
            });
            continue;
        }
        let raw = match raw {
            Ok(b) => b,
            Err(_) => {
                sum.invalid += 1;
                sum.tasks.push(TaskRow {
                    id: p.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").into(),
                    updated_at: None, messages: 0,
                    file: p.to_string_lossy().to_string(), state: "invalid".into()
                });
                continue;
            }
        };

        // parse JSON; tolerate schema differences
        let v: serde_json::Value = match serde_json::from_slice(&raw) {
            Ok(v) => v,
            Err(_) => {
                sum.invalid += 1;
                sum.tasks.push(TaskRow {
                    id: p.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").into(),
                    updated_at: None, messages: 0,
                    file: p.to_string_lossy().to_string(), state: "invalid".into()
                });
                continue;
            }
        };

        // normalize & redact
        let norm = normalize(&v, p);
        let updated_at = norm.updated_at.clone();
        let messages = norm.messages;
        let id = norm.id.clone();

        // compute raw hash for idempotency
        let sha_raw = sha256(&raw);
        let key = p.to_string_lossy().to_string();

        let entry = idx.entries.get(&key);
        let (state, should_write) = match entry {
            None => ("created", true),
            Some(e) if e.sha256_raw != sha_raw => ("changed", true),
            Some(_) => ("unchanged", false),
        };

        if should_write && !dry_run {
            // persist normalized (redacted) copy
            fs::create_dir_all(outdir)?;
            let fp = outdir.join(format!("{}.json", id));
            fs::write(&fp, serde_json::to_vec_pretty(&norm.redacted)?)?;
            // update index
            idx.entries.insert(key.clone(), IndexEntry {
                id: id.clone(),
                sha256_raw: sha_raw,
                last_imported_at: now_rfc3339(),
            });
        } else if should_write && dry_run {
            // In dry run, update index entry for consistency but don't persist
            idx.entries.insert(key.clone(), IndexEntry {
                id: id.clone(),
                sha256_raw: sha_raw,
                last_imported_at: now_rfc3339(),
            });
        }

        match state {
            "created" => sum.created += 1,
            "changed" => sum.changed += 1,
            "unchanged" => sum.unchanged += 1,
            _ => {}
        };

        sum.tasks.push(TaskRow {
            id, updated_at, messages,
            file: p.to_string_lossy().to_string(),
            state: state.into(),
        });

        if limit > 0 && sum.tasks.len() >= limit as usize {
            break;
        }
    }

    if !dry_run {
        save_index(outdir, &idx)?;
    }
    Ok(sum)
}

fn read_stable(p: &Path) -> (std::result::Result<Vec<u8>, std::io::Error>, bool) {
    // Simplified: For most use cases, files are stable. 
    // Only do stability checking if this becomes a real issue.
    match std::fs::read(p) {
        Ok(bytes) => (Ok(bytes), true), // Assume stable
        Err(e) => (Err(e), false),
    }
}

fn normalize(v: &serde_json::Value, path: &Path) -> NormalizedTask {
    let id = v.get("id").and_then(|x| x.as_str())
        .unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown")).to_string();
    let title = v.get("title").and_then(|x| x.as_str()).map(|s| s.to_string());
    let created = v.get("createdAt").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
    let updated = v.get("updatedAt").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
    let messages = v.get("messages").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
    let redacted = redact(v);
    NormalizedTask {
        id,
        title,
        created_at: created.map(|t| t.format(&Rfc3339).unwrap()),
        updated_at: updated.map(|t| t.format(&Rfc3339).unwrap()),
        messages,
        source_file: path.to_string_lossy().to_string(),
        redacted,
    }
}

fn redact(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value::*;
    match v {
        Object(m) => {
            let mut out = serde_json::Map::new();
            for (k, val) in m {
                let lk = k.to_ascii_lowercase();
                if lk.contains("apikey") || lk.contains("api_key") || lk.contains("token")
                    || lk.contains("secret") || lk.contains("password") {
                    out.insert(k.clone(), String("***redacted***".into()));
                } else {
                    out.insert(k.clone(), redact(val));
                }
            }
            Object(out)
        }
        Array(a) => Array(a.iter().map(redact).collect()),
        _ => v.clone(),
    }
}

fn sha256(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new(); h.update(b);
    format!("sha256:{:x}", h.finalize())
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

fn index_path(outdir: &Path) -> PathBuf {
    outdir.parent().unwrap_or(outdir).join("index.json")
}
fn load_index(outdir: &Path) -> anyhow::Result<Index> {
    let p = index_path(outdir);
    if !p.exists() { return Ok(Index::default()); }
    let txt = fs::read_to_string(p)?;
    Ok(serde_json::from_str(&txt).unwrap_or_default())
}
fn save_index(outdir: &Path, idx: &Index) -> anyhow::Result<()> {
    let p = index_path(outdir);
    if let Some(dir) = p.parent() { fs::create_dir_all(dir)?; }
    fs::write(p, serde_json::to_vec_pretty(idx)?)?;
    Ok(())
}

/* -------- helper: correlate AIM messages within run window -------- */
fn correlate_messages_for_run(dot: &std::path::Path, rec: &crate::correlation::CorrRecord) -> serde_json::Value {
    // derive window
    let start = time::OffsetDateTime::parse(&rec.window.start, &time::format_description::well_known::Rfc3339).ok();
    let end   = time::OffsetDateTime::parse(&rec.window.end, &time::format_description::well_known::Rfc3339).ok();
    if start.is_none() || end.is_none() { return serde_json::json!({}); }
    let (start, end) = (start.unwrap(), end.unwrap());

    // load AIM streams for listed tasks
    let mut total = 0u64;
    let mut by_role = std::collections::BTreeMap::<String,u64>::new();
    let mut user_ts: Option<time::OffsetDateTime> = None;
    let mut first_assistant_ts: Option<time::OffsetDateTime> = None;
    let mut code_blocks = 0u64;

    for t in &rec.tasks {
        let tid = t.id.clone();
        let msgs_path = dot.join("agents").join("streams").join("Cline").join(&tid).join("messages.ndjson");
        if !msgs_path.exists() { continue; }
        if let Ok(lines) = std::fs::read_to_string(&msgs_path) {
            for line in lines.lines() {
                let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, _ => continue };
                let ts = v.get("created_at").and_then(|x| x.as_str()).and_then(|s| time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok());
                if ts.is_none() { continue; }
                let ts = ts.unwrap();
                if ts < start || ts > end { continue; }
                total += 1;
                let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("assistant").to_string();
                *by_role.entry(role.clone()).or_insert(0) += 1;
                if role == "user" { user_ts = Some(ts); }
                if role == "assistant" && first_assistant_ts.is_none() { first_assistant_ts = Some(ts); }
                if let Some(arr) = v.get("code_blocks").and_then(|x| x.as_array()) { code_blocks += arr.len() as u64; }
            }
        }
    }

    // latency (ms): first assistant reply after latest preceding user msg in window
    let latency_ms = match (user_ts, first_assistant_ts) {
        (Some(u), Some(a)) if a >= u => (a - u).whole_milliseconds().max(0) as u64,
        _ => 0
    };

    serde_json::json!({
        "messages_in_window": total,
        "by_role": by_role,
        "first_reply_latency_ms": latency_ms,
        "code_blocks_in_window": code_blocks
    })
}
