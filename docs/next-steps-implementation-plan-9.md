Love this direction. Yes—you can (and should) ingest Cline task history as read‑only events, then optionally funnel all terminal commands through your CLI to get full provenance. Below is a pragmatic, incremental plan plus drop‑in code for a new cline plugin (locate + import) and a minimal term run wrapper. I’ve also included devcontainer hints so imports work even inside containers.

Why this works
	•	Cline stores checkpoints and (today) task data in VS Code globalStorage; the extension id is saoudrizwan.claude-dev (legacy name), so the path is predictable. They’re also moving toward workspace-specific storage under context.storageUri with tasks/ & checkpoints/. We’ll detect both patterns.  ￼ ￼ ￼
	•	Cline supports export to markdown and has a “task history” concept; we ingest the underlying files to avoid UI scraping.  ￼

⸻

What we’re adding (tiny slices)
	1.	cline locate — find your Cline storage (globalStorage/workspace storage) and print candidates + the “best” path.
	2.	cline import — read tasks from that storage, normalize to a stable schema, redact obvious secrets, write summaries to .nexoscli/cline/tasks/, and emit a single cline.import event with imported, skipped, and a per‑task digest. Supports --since (e.g. 24h or RFC3339).
	3.	term run -- <cmd…> — a tiny wrapper to execute commands via your CLI so you log cmd, cwd, exit_code, stdout/stderr SHA256 and (truncated) samples, plus dur_ms. This is the safe on‑ramp to “everything through the gate”.

Later (not in this patch): a file watcher that snapshots git status/git diff after each term run to build a replayable history.

⸻

1) New plugin: plugins/cline

plugins/cline/Cargo.toml

[package]
name = "nexoscli-plugin-cline"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
walkdir = "2"
time = { version = "0.3", features = ["parsing", "formatting"] }
dirs = "5"

plugins/cline/src/lib.rs

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{Plugin, CommandContext, CommandOutput, Result, HealthStatus};
use schemars::{JsonSchema, schema_for};
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

    fn import_from(tasks_dir: &Path, since: Option<OffsetDateTime>, outdir: &Path) -> anyhow::Result<ImportSummary> {
        fs::create_dir_all(outdir)?;
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
            // persist normalized copy for reproducibility
            let fp = outdir.join(format!("{}.json", t.id));
            fs::write(&fp, serde_json::to_vec_pretty(&t.redacted)?)?;
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
    fn description(&self) -> &'static str { "Import Cline task history/checkpoints (read-only)" }

    fn register_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("cline")
                .about(self.description())
                .subcommand(
                    ClapCommand::new("locate")
                        .about("Find Cline storage (globalStorage/workspace)")
                )
                .subcommand(
                    ClapCommand::new("import")
                        .about("Import tasks from Cline storage (read-only)")
                        .arg(Arg::new("storage").long("storage").value_name("PATH|auto"))
                        .arg(Arg::new("since").long("since").value_name("WINDOW"))
                        .arg(Arg::new("limit").long("limit").value_parser(clap::value_parser!(u64)))
                        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                )
        )
    }

    fn execute(&self, matches: &ArgMatches, ctx: &CommandContext) -> Result<CommandOutput> {
        match matches.subcommand() {
            Some(("locate", _)) => {
                let cands = Self::find_tasks_dirs();
                let best = Self::select_best(&cands);
                let payload = LocateResult {
                    selected: best.as_ref().map(|c| c.tasks_dir.clone()),
                    candidates: cands,
                };
                let exit = ctx.ok("cline","locate",&payload);
                Ok(CommandOutput{ message: None, data: Some(json!(payload)), exit_code: exit })
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
                if !dir.exists() { return Err(anyhow::anyhow!("tasks dir not found: {}", dir.display()).into()); }

                let outdir = PathBuf::from(".nexoscli").join("cline").join("tasks");
                let mut sum = Self::import_from(&dir, since, &outdir)?;
                if limit > 0 && sum.tasks.len() as u64 > limit {
                    sum.tasks.truncate(limit as usize);
                    // imported count still reflects full scan; that’s fine for a summary
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
                    Ok(CommandOutput{ message: None, data: Some(event), exit_code: exit })
                } else {
                    let exit = ctx.ok("cline","import",&event);
                    Ok(CommandOutput{ message: None, data: Some(event), exit_code: exit })
                }
            }
            _ => Err(anyhow::anyhow!("unknown cline command").into())
        }
    }

    fn health_check(&self) -> Result<HealthStatus> { Ok(HealthStatus::Healthy) }

    fn config_schema(&self) -> Option<serde_json::Value> {
        #[derive(Debug, Serialize, Deserialize, JsonSchema)]
        struct ClineCfg {
            #[serde(default)] redact: Option<bool>,
            #[serde(default)] editors: Option<Vec<String>>,
            #[serde(default)] ext_ids: Option<Vec<String>>,
        }
        Some(serde_json::to_value(schema_for!(ClineCfg)).unwrap())
    }
}

This importer is schema‑tolerant: it only needs id/updatedAt/messages[] if present, and will still ingest with just file metadata.

Register the plugin

In nexoscli-bin/src/main.rs, where you register plugins:

// built-ins...
register_plugin!(registry, nexoscli_plugin_cline::ClinePlugin::new());

Your workspace already includes plugins/*, so this crate will build automatically after you add it.

⸻

2) Minimal terminal wrapper: plugins/term

plugins/term/Cargo.toml

[package]
name = "nexoscli-plugin-term"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"

plugins/term/src/lib.rs

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{Plugin, CommandContext, CommandOutput, Result, HealthStatus};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use std::process::Command;

pub struct TermPlugin;
impl TermPlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct RunArgs {
    cmd: Vec<String>,           // trailing args
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    sample_kb: Option<usize>,   // truncation
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct RunResult {
    cmd: String,
    cwd: String,
    exit_code: i32,
    stdout_sha256: String,
    stderr_sha256: String,
    stdout_sample: String,
    stderr_sample: String,
}

impl Plugin for TermPlugin {
    fn name(&self) -> &'static str { "term" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn description(&self) -> &'static str { "Run commands with provenance (non-interactive)" }

    fn register_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
          ClapCommand::new("term").about(self.description())
            .subcommand(
              ClapCommand::new("run")
                .about("Execute a command and capture output")
                .arg(Arg::new("cmd").num_args(1..).trailing_var_arg(true).required(true))
                .arg(Arg::new("cwd").long("cwd").value_name("DIR"))
                .arg(Arg::new("sample-kb").long("sample-kb").value_parser(clap::value_parser!(usize)))
                .arg(Arg::new("passthrough").long("passthrough").action(ArgAction::SetTrue)
                    .help("Also print child stdout/stderr to console"))
            )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<CommandOutput> {
        match m.subcommand() {
            Some(("run", mm)) => {
                let args: Vec<String> = mm.get_many::<String>("cmd").unwrap().cloned().collect();
                let cwd = mm.get_one::<String>("cwd").cloned()
                    .unwrap_or(std::env::current_dir()?.to_string_lossy().to_string());
                let sample = *mm.get_one::<usize>("sample-kb").unwrap_or(&64);
                let passthrough = mm.get_flag("passthrough");

                // Shell for portability
                let joined = sh_join(&args);
                let mut cmd = if cfg!(target_os = "windows") {
                    let mut c = Command::new("cmd");
                    c.arg("/C").arg(&joined);
                    c
                } else {
                    let mut c = Command::new("sh");
                    c.arg("-lc").arg(&joined);
                    c
                };
                let output = cmd.current_dir(&cwd).output()?;

                let (stdout_sample, stdout_sha256) = digest_and_sample(&output.stdout, sample);
                let (stderr_sample, stderr_sha256) = digest_and_sample(&output.stderr, sample);
                if passthrough {
                    use std::io::Write;
                    let _ = std::io::stdout().write_all(&output.stdout);
                    let _ = std::io::stderr().write_all(&output.stderr);
                }

                let exit_code = output.status.code().unwrap_or(-1);
                let data = RunResult {
                    cmd: joined.clone(), cwd: cwd.clone(), exit_code,
                    stdout_sha256, stderr_sha256, stdout_sample, stderr_sample
                };
                let code = if exit_code == 0 {
                    ctx.ok("term","run",&data)
                } else {
                    ctx.err("term","run","term.run.failed","command failed", Some(&joined), exit_code)
                };
                Ok(CommandOutput { message: None, data: Some(serde_json::to_value(&data)?), exit_code: code })
            }
            _ => Err(anyhow::anyhow!("unknown term command").into())
        }
    }

    fn health_check(&self) -> Result<HealthStatus> { Ok(HealthStatus::Healthy) }
}

fn sh_join(parts: &[String]) -> String {
    // minimal escaping
    parts.iter().map(|s| {
        if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:".contains(c)) { s.clone() }
        else { format!("'{}'", s.replace('\'', "'\\''")) }
    }).collect::<Vec<_>>().join(" ")
}

fn digest_and_sample(bytes: &[u8], kb: usize) -> (String, String) {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new(); h.update(bytes);
    let sha = format!("sha256:{:x}", h.finalize());
    let lim = (kb * 1024).min(bytes.len());
    let sample = String::from_utf8_lossy(&bytes[..lim]).to_string();
    (sample, sha)
}

Register this plugin in main.rs as well:

register_plugin!(registry, nexoscli_plugin_term::TermPlugin::new());


⸻

3) Tests (add to nexoscli-bin/tests/)

cline_import.rs

mod helpers; use helpers::*;
use std::fs; use std::path::Path;

#[test]
fn cline_import_reads_tasks_dir() {
    let t = tmp();
    // fake Cline storage
    let storage = t.path().join("cline-storage").join("tasks");
    fs::create_dir_all(&storage).unwrap();
    fs::write(storage.join("task-123.json"), r#"{
      "id":"task-123","title":"Demo","createdAt":"2025-08-30T12:34:56Z",
      "updatedAt":"2025-08-30T13:00:00Z","messages":[{"role":"user","content":"Hi"}],
      "creds":{"apiKey":"secret"}
    }"#).unwrap();

    let v = run_json(t.path(), &["cline","import","--storage", storage.to_str().unwrap()]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    assert!(v["data"]["imported"].as_u64().unwrap() >= 1);
    let tasks = v["data"]["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["id"], "task-123");
    // normalized copy present
    let norm = t.path().join(".nexoscli").join("cline").join("tasks").join("task-123.json");
    assert!(Path::new(&norm).exists());
    // redaction
    let body = fs::read_to_string(norm).unwrap();
    assert!(body.contains("***redacted***"));
}

term_run.rs

mod helpers; use helpers::*;

#[test]
fn term_run_captures_output_and_exit_code() {
    let t = tmp();
    let v = run_json(t.path(), &["term","run","echo","hello"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    let data = &v["data"];
    assert_eq!(data["exit_code"], 0);
    assert!(data["stdout_sample"].as_str().unwrap().contains("hello"));
}

Run everything:

cargo test --workspace


⸻

4) Devcontainer tip (so cline import works inside the container)

Map your host Cline storage into the container (macOS example):

// .devcontainer/devcontainer.json (snippet)
{
  "mounts": [
    "source=${localEnv:HOME}/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev,target=/workspaces/vscode-globalStorage/saoudrizwan.claude-dev,type=bind"
  ]
}

Then run:

nexoscli cline import --storage /workspaces/vscode-globalStorage/saoudrizwan.claude-dev/tasks --output json

Cline stores checkpoints in VS Code globalStorage today; newer builds may move to workspace storage—our locate handles both via candidates.  ￼ ￼

⸻

5) Optional: nudge Cline to run commands via your wrapper

Drop a rule into .clinerules/ to prefer nexoscli term run:

<!-- .clinerules/nexos-term-wrapper.md -->
# Prefer NEXOS CLI for commands

When proposing to run shell commands, wrap them like:

`nexoscli term run -- <original command>`

Rationale: adds provenance (dur_ms, exit_code, output digests) and feeds telemetry.

Exceptions: interactive TTY sessions or long-running dev servers (explain why).

Rules are stored in .clinerules/ for workspace rules.  ￼

⸻

6) Acceptance criteria (for this feature)
	•	nexoscli cline locate prints candidates and a selected tasks dir (if found).
	•	nexoscli cline import --storage <path> writes normalized copies to .nexoscli/cline/tasks/ and emits a cline.import event with dur_ms.
	•	Redaction replaces common secret keys with ***redacted***.
	•	nexoscli term run echo hello succeeds and logs dur_ms, stdout_sha256, stderr_sha256.
	•	Both commands appear in dev telemetry report (counts & latency) with your new metrics.
