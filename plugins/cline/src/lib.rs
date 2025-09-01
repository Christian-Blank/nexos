use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::{plugin::{Plugin, CommandContext}, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::{Path, PathBuf}};
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
use walkdir::WalkDir;

pub struct ClinePlugin;

impl ClinePlugin {
    pub fn new() -> Self { Self }

    fn candidate_global_storage_dirs() -> Vec<PathBuf> {
        let mut out = Vec::new();
        let home = dirs::home_dir();

        #[cfg(target_os = "macos")]
        let editors = ["Code","Code - Insiders","Cursor","VSCodium","Windsurf"];
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let editors = ["Code","Code - Insiders","VSCodium","code-oss","Cursor","Windsurf"];
        #[cfg(target_os = "windows")]
        let editors = ["Code","Code - Insiders","VSCodium","Cursor","Windsurf"];

        if let Some(home) = home {
            #[cfg(target_os = "macos")]
            let base = home.join("Library").join("Application Support");
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            let base = dirs::config_dir().unwrap_or(home.join(".config"));
            #[cfg(target_os = "windows")]
            let base = dirs::data_dir().unwrap_or(home.join("AppData").join("Roaming"));

            for ed in editors {
                let p = base.join(ed).join("User").join("globalStorage");
                if p.exists() { out.push(p); }
            }
        }
        out
    }

    fn probable_ext_ids() -> Vec<&'static str> {
        // Legacy + possible future ids
        vec!["saoudrizwan.claude-dev","cline.cline"]
    }

    fn find_tasks_dirs() -> Vec<Candidate> {
        let mut cands = Vec::new();
        for root in Self::candidate_global_storage_dirs() {
            for ext_id in Self::probable_ext_ids() {
                let base = root.join(ext_id);
                let tdir = base.join("tasks");
                let cdir = base.join("checkpoints");
                let latest = newest_mtime(&tdir).or_else(|| newest_mtime(&cdir));
                cands.push(Candidate{
                    editor_root: root.clone(), ext_id: ext_id.to_string(),
                    tasks_dir: tdir, checkpoints_dir: cdir, latest_mtime: latest,
                });
            }
        }
        cands
    }

    fn select_best(cands: &[Candidate]) -> Option<Candidate> {
        let mut v = cands.to_vec();
        v.sort_by(|a,b| b.latest_mtime.cmp(&a.latest_mtime));
        v.into_iter().find(|c| c.tasks_dir.exists())
    }

    fn parse_since(s: Option<&String>) -> Option<OffsetDateTime> {
        match s {
            None => None,
            Some(s) if s.ends_with('h') => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
            Some(s) if s.ends_with('d') => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
            Some(s) => OffsetDateTime::parse(s, &Rfc3339).ok(),
        }
    }

    fn normalize_task(path: &Path) -> anyhow::Result<NormalizedTask> {
        let txt = fs::read_to_string(path)?;
        let v: serde_json::Value = serde_json::from_str(&txt)?;
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown")).to_string();
        let title = v.get("title").and_then(|x| x.as_str()).map(|s| s.to_string());
        let created = v.get("createdAt").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
        let updated = v.get("updatedAt").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
        let messages = v.get("messages").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);

        // redaction (simple, conservative)
        let redacted = redact_value(&v);

        Ok(NormalizedTask {
            id, title, created_at: created.map(|t| t.format(&Rfc3339).unwrap()),
            updated_at: updated.map(|t| t.format(&Rfc3339).unwrap()),
            messages,
            source_file: path.to_string_lossy().to_string(),
            redacted: redacted,
        })
    }

    fn import_from(tasks_dir: &Path, since: Option<OffsetDateTime>, outdir: &Path, dry_run: bool) -> anyhow::Result<ImportSummary> {
        if !dry_run {
            fs::create_dir_all(outdir)?;
        }
        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut list = Vec::<TaskRow>::new();

        for ent in WalkDir::new(tasks_dir).min_depth(1).max_depth(2) {
            let ent = match ent { Ok(e) => e, Err(_) => continue };
            let p = ent.path();
            if p.is_dir() { continue; }
            if p.extension().and_then(|s| s.to_str()) != Some("json") { continue; }

            // skip by mtime if since provided
            if let Some(since) = since {
                if let Ok(meta) = fs::metadata(p) {
                    if let Ok(modified) = meta.modified() {
                        let t = OffsetDateTime::from(modified);
                        if t < since { skipped += 1; continue; }
                    }
                }
            }

            let t = match Self::normalize_task(p) {
                Ok(t) => t, Err(_) => { skipped += 1; continue; }
            };
            
            // Only persist files if not in dry-run mode
            if !dry_run {
                let fp = outdir.join(format!("{}.json", t.id));
                fs::write(&fp, serde_json::to_vec_pretty(&t.redacted)?)?;
            }
            imported += 1;
            list.push(TaskRow{
                id: t.id, updated_at: t.updated_at, messages: t.messages, file: p.to_string_lossy().to_string()
            });
        }

        Ok(ImportSummary { imported: imported as u64, skipped: skipped as u64, tasks: list })
    }
}

fn newest_mtime(dir: &Path) -> Option<OffsetDateTime> {
    if !dir.exists() { return None; }
    let mut best = None;
    for ent in WalkDir::new(dir).min_depth(1).max_depth(2) {
        let ent = ent.ok()?;
        if ent.path().is_dir() { continue; }
        if let Ok(meta) = ent.metadata() {
            if let Ok(m) = meta.modified() {
                let ts = OffsetDateTime::from(m);
                if best.map(|b| ts > b).unwrap_or(true) { best = Some(ts); }
            }
        }
    }
    best
}

fn redact_value(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value::{Object, Array, String as JStr};
    match v {
        Object(map) => {
            let mut out = serde_json::Map::new();
            for (k,val) in map {
                let lk = k.to_ascii_lowercase();
                if lk.contains("apikey") || lk.contains("api_key") || lk.contains("token") || lk.contains("secret") || lk.contains("password") {
                    out.insert(k.clone(), JStr("***redacted***".into()));
                } else {
                    out.insert(k.clone(), redact_value(val));
                }
            }
            serde_json::Value::Object(out)
        }
        Array(a) => serde_json::Value::Array(a.iter().map(redact_value).collect()),
        _ => v.clone(),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct Candidate {
    editor_root: PathBuf,
    ext_id: String,
    tasks_dir: PathBuf,
    checkpoints_dir: PathBuf,
    #[serde(skip_serializing_if="Option::is_none")]
    #[schemars(skip)]
    latest_mtime: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct LocateResult {
    selected: Option<PathBuf>,
    candidates: Vec<Candidate>,
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
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ImportSummary {
    imported: u64,
    skipped: u64,
    tasks: Vec<TaskRow>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ImportArgs {
    #[serde(default)]
    storage: Option<String>, // "auto" | explicit path
    #[serde(default)]
    since: Option<String>,   // "24h" | "7d" | RFC3339
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    dry_run: bool,
}

impl Plugin for ClinePlugin {
    fn name(&self) -> &'static str { "cline" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Import Cline task history/checkpoints (read-only)" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
            Command::new("cline")
                .about(self.describe())
                .subcommand(
                    Command::new("locate")
                        .about("Find Cline storage (globalStorage/workspace)")
                )
                .subcommand(
                    Command::new("import")
                        .about("Import tasks from Cline storage (read-only)")
                        .arg(Arg::new("storage").long("storage").value_name("PATH|auto"))
                        .arg(Arg::new("since").long("since").value_name("WINDOW"))
                        .arg(Arg::new("limit").long("limit").value_parser(clap::value_parser!(u64)))
                        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                )
        )
    }

    fn execute(&self, matches: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match matches.subcommand() {
            Some(("locate", _)) => {
                let cands = Self::find_tasks_dirs();
                let best = Self::select_best(&cands);
                let payload = LocateResult {
                    selected: best.as_ref().map(|c| c.tasks_dir.clone()),
                    candidates: cands,
                };
                let exit = ctx.ok("cline","locate",&payload);
                Ok(exit)
            }
            Some(("import", m)) => {
                let since = Self::parse_since(m.get_one::<String>("since"));
                let storage = m.get_one::<String>("storage").cloned().unwrap_or_else(|| "auto".into());
                let limit = *m.get_one::<u64>("limit").unwrap_or(&0);
                let dry = m.get_flag("dry-run");

                let dir = if storage == "auto" {
                    let cands = Self::find_tasks_dirs();
                    let best = Self::select_best(&cands).ok_or_else(|| anyhow::anyhow!("no Cline tasks dir found; run `nexoscli cline locate`"))?;
                    best.tasks_dir
                } else {
                    PathBuf::from(storage)
                };
                if !dir.exists() { 
                    return Ok(ctx.err("cline","import","cline.import.notfound","tasks dir not found", Some(&format!("directory: {}", dir.display())), 1));
                }

                let outdir = PathBuf::from(".nexoscli").join("cline").join("tasks");
                let mut sum = Self::import_from(&dir, since, &outdir, dry)?;
                if limit > 0 && sum.tasks.len() as u64 > limit {
                    sum.tasks.truncate(limit as usize);
                    // imported count still reflects full scan; that's fine for a summary
                }

                let event = json!({
                  "storage": dir,
                  "since": since.map(|s| s.format(&Rfc3339).unwrap()),
                  "imported": sum.imported,
                  "skipped": sum.skipped,
                  "persisted_dir": outdir,
                  "tasks": sum.tasks,
                });

                if dry {
                    let exit = ctx.ok("cline","import.dryrun",&event);
                    Ok(exit)
                } else {
                    let exit = ctx.ok("cline","import",&event);
                    Ok(exit)
                }
            }
            _ => Ok(ctx.err("cline", "unknown", "cline.unknown", "unknown cline command", None, 2))
        }
    }
}
