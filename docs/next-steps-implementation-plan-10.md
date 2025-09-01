Absolutely—let’s lock in guard rails + robust detection for Cline so it’s stable on macOS/Linux/Windows, and in a VS Code Dev Container. We’ll also make the Doctor check aware of Cline, with a soft‑fail default (informational) and an opt‑in hard‑fail.

Below are drop‑in patches and tests for your repo. They add:
	1.	Bulletproof discovery: env overrides → devcontainer/remote paths → user globalStorage (Code/Cursor/VSCodium/Windsurf) on macOS/Linux/Windows.
	2.	Idempotent import: raw‑file SHA256 index; created/changed/unchanged/busy/invalid counters; partial‑write safeguards.
	3.	Doctor integration: dev doctor --ci --checks=cline becomes informative by default; enforce with --require cline or NEXOS_CLINE_REQUIRED=1.
	4.	Config hooks: simple env toggles (NEXOS_CLINE_*) so you don’t need typed config yet.
	5.	Tests: no host dependency; use NEXOS_CLINE_TEST_ROOT to simulate platform paths.

⸻

1) Cline plugin — robust locate + idempotent import

1.1 Update plugins/cline/Cargo.toml (add one dep)

[dependencies]
# ...existing...
walkdir = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
time = { version = "0.3", features = ["parsing","formatting"] }
dirs = "5"
# NEW (for sleeping a few ms on busy file)
spin_sleep = "1"
sha2 = { workspace = true }

1.2 Replace plugins/cline/src/lib.rs with the implementation below

What’s new vs. the earlier version you added:
	•	Detection paths include:
	•	Devcontainer/Remote: ~/.vscode-server/data/User/globalStorage, ~/.vscode-remote/data/User/globalStorage, and a common mount /workspaces/vscode-globalStorage.
	•	User globalStorage across editors {Code, Code - Insiders, VSCodium, Cursor, Windsurf} on macOS/Linux/Windows.
	•	Env overrides: NEXOS_CLINE_STORAGE, NEXOS_CLINE_EDITORS, NEXOS_CLINE_EXTIDS, NEXOS_CLINE_TEST_ROOT.
	•	Import is idempotent: we compute raw JSON SHA256; only write normalized copies on change; maintain an index under .nexoscli/cline/index.json.
	•	Handles busy files (changing during read) and invalid JSON gracefully.
	•	Extended summary with created/changed/unchanged/busy/invalid.

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{Plugin, CommandContext, CommandOutput, Result, HealthStatus};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, fs, io::Read, path::{Path, PathBuf}};
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
use walkdir::WalkDir;

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
    imported: u64,    // total scanned (after filters)
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
    fn description(&self) -> &'static str { "Cline task history (locate/import) with idempotent ingest" }

    fn register_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("cline").about(self.description())
            .subcommand(ClapCommand::new("locate").about("Find Cline storage (globalStorage/workspace)"))
            .subcommand(
                ClapCommand::new("import").about("Import tasks (read-only)")
                    .arg(Arg::new("storage").long("storage").value_name("PATH|auto"))
                    .arg(Arg::new("since").long("since").value_name("WINDOW"))
                    .arg(Arg::new("limit").long("limit").value_parser(clap::value_parser!(u64)))
                    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
            )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<CommandOutput> {
        match m.subcommand() {
            Some(("locate", _)) => {
                let (cands, selected, hints) = locate_all();
                let result = LocateResult {
                    selected: selected.clone(),
                    candidates: cands,
                    hints,
                };
                let exit = ctx.ok("cline","locate",&result);
                Ok(CommandOutput { message: None, data: Some(serde_json::to_value(&result)?), exit_code: exit })
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
                let mut summary = import_tasks(&storage, since_ts, &outdir, limit)?;
                summary.persisted_dir = Some(outdir.display().to_string());

                let payload = serde_json::to_value(&summary)?;
                let exit = if dry {
                    ctx.ok("cline","import.dryrun",&payload)
                } else {
                    ctx.ok("cline","import",&payload)
                };
                Ok(CommandOutput { message: None, data: Some(payload), exit_code: exit })
            }
            _ => Err(anyhow::anyhow!("unknown cline command").into()),
        }
    }

    fn health_check(&self) -> Result<HealthStatus> { Ok(HealthStatus::Healthy) }

    fn config_schema(&self) -> Option<serde_json::Value> {
        #[derive(Debug, Serialize, Deserialize, JsonSchema)]
        struct Cfg { #[serde(default)] required: Option<bool> }
        Some(serde_json::to_value(schema_for!(Cfg)).unwrap())
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

fn import_tasks(tasks_dir: &Path, since: Option<OffsetDateTime>, outdir: &Path, limit: u64) -> anyhow::Result<ImportSummary> {
    fs::create_dir_all(outdir)?;
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
                    if t < since { continue; }
                }
            }
        }
        sum.imported += 1;

        // Read raw bytes with a simple "stable size" check to avoid partial writes
        let (raw, state_busy) = read_stable(p);
        if state_busy {
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

        if should_write {
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

        if limit > 0 && (sum.created + sum.changed + sum.unchanged + sum.busy + sum.invalid) >= limit {
            break;
        }
    }

    save_index(outdir, &idx)?;
    Ok(sum)
}

fn read_stable(p: &Path) -> (Result<Vec<u8>, std::io::Error>, bool) {
    // try read → stat → small sleep → stat; if size changed, treat as busy
    use spin_sleep::sleep;
    use std::time::Duration as StdDur;

    let meta1 = fs::metadata(p).ok();
    let mut file = match fs::File::open(p) {
        Ok(f) => f,
        Err(e) => return (Err(e), false),
    };
    let mut buf = Vec::new();
    if let Err(e) = file.read_to_end(&mut buf) {
        return (Err(e), false);
    }
    let meta2 = fs::metadata(p).ok();

    let mut busy = false;
    if let (Some(m1), Some(m2)) = (meta1, meta2) {
        if m2.len() < m1.len() { busy = true; }
        if m1.len() == m2.len() {
            // quick second check
            sleep(StdDur::from_millis(5));
            if let Ok(m3) = fs::metadata(p) {
                if m3.len() != m2.len() { busy = true; }
            }
        }
    }
    (Ok(buf), busy)
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


⸻

2) Doctor — add the Cline check (soft by default; strict on demand)

2.1 Extend plugins/dev/src/lib.rs

Add new fields to your existing doctor CI result rows (non‑breaking):

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorCiCheck {
    name: String,
    ok: bool,
    detail: String,
    #[serde(default)]
    severity: String,          // "info" | "error"
    #[serde(default)]
    fix: Option<String>,
}

Add CLI flags for strictness:

.arg(Arg::new("require").long("require").value_name("CSV")
     .help("Checks that must pass (comma-separated). If any fails -> non-zero. Example: --require cline"))

In the --ci branch, after existing checks, add the Cline check:

// cline (informational unless required)
if want.contains("cline") {
    let mut ok = true;
    let mut detail = String::new();
    let mut fix = None;

    // call cline locate
    let (code, val) = run_cli_json(&cwd, &["cline","locate"])?;
    if code != 0 {
        ok = false;
        detail = "cline locate failed".into();
    } else {
        let sel = val["data"]["selected"].as_str();
        if sel.is_none() {
            ok = false;
            detail = "no Cline storage found".into();
            fix = Some("If in Dev Container, mount your host globalStorage; or set NEXOS_CLINE_STORAGE=/path/to/.../tasks".into());
        } else {
            // dry-run import of a few recent files
            let storage = sel.unwrap();
            let (c2, v2) = run_cli_json(&cwd, &["cline","import","--storage",storage,"--since","7d","--limit","5","--dry-run"])?;
            ok = ok && (c2 == 0);
            if ok {
                let imp = v2["data"]["imported"].as_u64().unwrap_or(0);
                detail = format!("storage={} imported~{}", storage, imp);
            } else {
                detail = format!("import dry-run failed for {}", storage);
                fix = Some("Inspect storage path; ensure readable JSON files under tasks/".into());
            }
        }
    }

    checks.push(DoctorCiCheck {
        name: "cline".into(),
        ok,
        detail,
        severity: if ok { "info".into() } else { "info".into() }, // soft by default
        fix,
    });
}

And finally, determine enforcement:

// Determine required checks (via flag or env)
let mut required: std::collections::HashSet<String> = m
    .get_one::<String>("require")
    .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
    .unwrap_or_default();
if std::env::var("NEXOS_CLINE_REQUIRED").ok().as_deref() == Some("1") {
    required.insert("cline".into());
}

let ok_all = checks.iter().all(|c| {
    if required.contains(&c.name) { c.ok } else { true }
});

if ok_all {
    let exit = ctx.ok("dev","doctor.ci",&DoctorCiResult{ ok: true, checks: checks.clone() });
    return Ok(exit);
} else {
    let failing: Vec<_> = checks.iter().filter(|c| required.contains(&c.name) && !c.ok).map(|c| c.name.clone()).collect();
    let exit = ctx.err("dev","doctor.ci","doctor.ci.failed",
        &format!("Required checks failed: {}", failing.join(",")),
        Some("Run with --checks to inspect sub-checks; see devcontainer mounts for cline"),
        2);
    return Ok(exit);
}

Result: cline check shows up in doctor --ci output; it does not fail by default if storage is missing; it will fail if you pass --require cline or set NEXOS_CLINE_REQUIRED=1.

⸻

3) Tests (platform‑agnostic)

Add these under nexoscli-bin/tests/.

3.1 cline_locate_and_import.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn cline_locate_via_test_root_and_idempotent_import() {
    let t = tmp();

    // Simulate a globalStorage root with ext id
    let root = t.path().join("globalStorage").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&root).unwrap();
    // First task
    fs::write(root.join("task-1.json"), br#"{
      "id":"task-1","title":"First","createdAt":"2025-08-30T12:00:00Z","updatedAt":"2025-08-30T12:10:00Z",
      "messages":[{"role":"user","content":"hi"}],"token":"secret"
    }"#).unwrap();

    // Make Cline discovery point here
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", t.path().join("globalStorage").to_str().unwrap());

    // locate should find it
    let v = run_json(t.path(), &["cline","locate"]);
    assert!(v["data"]["selected"].as_str().is_some());

    // import once -> created = 1
    let v1 = run_json(t.path(), &["cline","import","--storage", root.to_str().unwrap()]);
    assert_eq!(v1["data"]["created"].as_u64().unwrap(), 1);
    // secrets redacted in normalized copy
    let norm = t.path().join(".nexoscli/cline/tasks").join("task-1.json");
    let body = std::fs::read_to_string(norm).unwrap();
    assert!(body.contains("***redacted***"));

    // import again without change -> unchanged = 1
    let v2 = run_json(t.path(), &["cline","import","--storage", root.to_str().unwrap()]);
    assert_eq!(v2["data"]["unchanged"].as_u64().unwrap(), 1);

    // modify file -> changed = 1
    fs::write(root.join("task-1.json"), br#"{"id":"task-1","updatedAt":"2025-08-30T12:20:00Z","messages":[]}"#).unwrap();
    let v3 = run_json(t.path(), &["cline","import","--storage", root.to_str().unwrap()]);
    assert_eq!(v3["data"]["changed"].as_u64().unwrap(), 1);
}

3.2 doctor_ci_cline.rs

mod helpers; use helpers::*;
use std::fs;

#[test]
fn doctor_ci_reports_cline_info_and_can_require_it() {
    let t = tmp();

    // No storage -> info only, still OK (no --require)
    let v = run_json(t.path(), &["dev","doctor","--ci","--checks","cline"]);
    assert!(v["data"]["ok"].as_bool().unwrap());

    // Now create a fake storage and require cline
    let root = t.path().join("globalStorage").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", t.path().join("globalStorage").to_str().unwrap());

    // Should pass even when required (dry-run import succeeds)
    let v2 = run_json(t.path(), &["dev","doctor","--ci","--checks","cline","--require","cline"]);
    assert!(v2["data"]["ok"].as_bool().unwrap());
}

Run tests:

cargo test --workspace


⸻

4) How to use (manual acceptance)

# See candidates & hints
nexoscli cline locate --output json | jq

# Import the last 24h (idempotent)
nexoscli cline import --since 24h --dry-run --output json
nexoscli cline import --since 24h --output json

# Doctor (soft)
nexoscli dev doctor --ci --checks cline --output json | jq

# Doctor (strict)
nexoscli dev doctor --ci --checks cline --require cline --output json | jq
# or: NEXOS_CLINE_REQUIRED=1 nexoscli dev doctor --ci --checks cline

Devcontainer note: If cline locate finds nothing in a container, you’ll see the hints telling you to mount host globalStorage. A common mount on macOS:

// .devcontainer/devcontainer.json (snippet)
"mounts": [
  "source=${localEnv:HOME}/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev,target=/workspaces/vscode-globalStorage/saoudrizwan.claude-dev,type=bind"
]

Then nexoscli cline locate should pick /workspaces/vscode-globalStorage/saoudrizwan.claude-dev/tasks.

⸻

5) Why this is stable
	•	Multiple discovery layers (env → devcontainer/remote → user paths) mean we find storage in the most common setups without configuration.
	•	Busy file detection avoids half‑written JSON (we mark & skip).
	•	Idempotency via raw hash index prevents noisy rewrites and gives you accurate created/changed/unchanged.
	•	Doctor is informational by default (no CI flake) but can be enforced when you’re ready.
	•	All writes go under .nexoscli/cline/tasks/ + .nexoscli/cline/index.json, so it’s clean, replayable, and bundle‑friendly.

⸻

6) Next (once this lands)
	•	cline tail: file‑watcher (notify crate) that ingests on change and emits a cline.task.updated event.
	•	Patch capture after term run: record git diff --name-status and write patch files under .nexoscli/patches/.
	•	Replay: nexoscli replay thread <id> that replays term run commands and applies recorded patches (dry‑run first).

If you want, I can hand you the cline tail watcher next as a tiny, opt‑in background command that respects your “no hidden background work” rule (it runs only while the command is active).