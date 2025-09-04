use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::{plugin::{Plugin, CommandContext}, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::process::Command as StdCommand;
use notify::{RecommendedWatcher, Watcher, Config, EventKind, event::{ModifyKind, CreateKind, RemoveKind, RenameMode}};
use globset::{Glob, GlobSetBuilder};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use std::{fs, io::{Write, Read}, path::{Path, PathBuf}, sync::mpsc, thread, time::Duration};

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
    fn describe(&self) -> &'static str { "Run commands with provenance (non-interactive)" }

    fn augment_commands(&self, app: Command) -> Command {
        app.subcommand(
          Command::new("term").about(self.describe())
            .subcommand(
              Command::new("run")
                .about("Execute a command and capture output")
                .arg(Arg::new("cmd").num_args(1..).trailing_var_arg(true).required(true))
                .arg(Arg::new("cwd").long("cwd").value_name("DIR"))
                .arg(Arg::new("sample-kb").long("sample-kb").value_parser(clap::value_parser!(usize)))
                .arg(Arg::new("passthrough").long("passthrough").action(ArgAction::SetTrue)
                    .help("Also print child stdout/stderr to console"))
                .arg(Arg::new("fs-topic").long("fs-topic").action(ArgAction::SetTrue)
                    .help("Enable file-change topic capture during run"))
                .arg(Arg::new("fs-root").long("fs-root").num_args(1..).value_name("PATH")
                    .help("Watch roots (repeatable). Defaults to current dir"))
                .arg(Arg::new("fs-exclude").long("fs-exclude").num_args(1..).value_name("GLOB")
                    .help("Exclude globs; defaults: .git/**, .nexoscli/**, target/**, node_modules/**"))
                .arg(Arg::new("fs-hash").long("fs-hash").action(ArgAction::SetTrue)
                    .help("Compute sha256 for Create/Modify"))
            )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            Some(("run", mm)) => {
                let args: Vec<String> = mm.get_many::<String>("cmd").unwrap().cloned().collect();
                let cwd = mm.get_one::<String>("cwd").cloned()
                    .unwrap_or(std::env::current_dir()?.to_string_lossy().to_string());
                let sample = *mm.get_one::<usize>("sample-kb").unwrap_or(&64);
                let passthrough = mm.get_flag("passthrough");

                // FS topic flags
                let fs_topic = mm.get_flag("fs-topic");
                let roots: Vec<PathBuf> = mm.get_many::<String>("fs-root")
                    .map(|vals| vals.map(PathBuf::from).collect())
                    .unwrap_or_else(|| vec![]);
                let excludes: Vec<String> = mm.get_many::<String>("fs-exclude")
                    .map(|vals| vals.cloned().collect())
                    .unwrap_or_else(|| vec![]);
                let do_hash = mm.get_flag("fs-hash");

                // Start watcher before spawning child
                let mut fs_handle: Option<FsTopicHandle> = None;
                if fs_topic {
                    fs_handle = Some(fs_topic_start(FsTopicCfg{ roots, excludes, hash: do_hash })?);
                }

                // Shell for portability
                let joined = sh_join(&args);
                let mut cmd = if cfg!(target_os = "windows") {
                    let mut c = StdCommand::new("cmd");
                    c.arg("/C").arg(&joined);
                    c
                } else {
                    let mut c = StdCommand::new("sh");
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

                // Stop watcher and emit fs.topic event if enabled
                if let Some(h) = fs_handle {
                    let (path, lines, bytes, sha, from_ts, to_ts) = fs_topic_stop(h)?;
                    let _ = ctx.events.write(
                        "term",
                        "fs.topic",
                        &serde_json::json!({
                            "file": path.display().to_string(),
                            "lines": lines,
                            "bytes": bytes,
                            "sha256": sha,
                            "from_ts": from_ts,
                            "to_ts": to_ts
                        }),
                        0, None, None, None
                    );
                }

                let data = RunResult {
                    cmd: joined.clone(), cwd: cwd.clone(), exit_code,
                    stdout_sha256, stderr_sha256, stdout_sample, stderr_sample
                };
                // Always return success with data, regardless of child process exit code
                // The exit code is part of the captured data, not a plugin error
                ctx.ok("term","run",&data);
                Ok(exit_code)  // Return the child's exit code as the plugin's exit code
            }
            _ => Ok(ctx.err("term", "unknown", "term.unknown", "unknown term command", None, 2))
        }
    }
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

// ----------------- FS Topic implementation -----------------

#[derive(Clone)]
struct FsTopicCfg {
    roots: Vec<PathBuf>,
    excludes: Vec<String>,
    hash: bool,
}

struct FsTopicHandle {
    path: PathBuf,
    join: Option<std::thread::JoinHandle<(u64,u64,String,String)>>, // (lines, bytes, from_ts, to_ts)
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn open_fileops_path() -> anyhow::Result<PathBuf> {
    let base = PathBuf::from(".nexoscli").join("fileops");
    fs::create_dir_all(&base)?;
    let ts = OffsetDateTime::now_utc().unix_timestamp();
    let pid = std::process::id();
    Ok(base.join(format!("fileops-{}-{}.ndjson", ts, pid)))
}

fn sha256_file(p: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Some(format!("sha256:{:x}", h.finalize()))
}

fn fs_topic_start(cfg: FsTopicCfg) -> anyhow::Result<FsTopicHandle> {
    let out_path = open_fileops_path()?;
    let (tx_ev, rx_ev) = mpsc::channel();
    let (tx_stop, rx_stop) = mpsc::channel::<()>();

    // Build glob excludes
    let mut gb = GlobSetBuilder::new();
    let mut exclude = cfg.excludes.clone();
    if exclude.is_empty() {
        exclude = vec![".git/**".into(), ".nexoscli/**".into(), "target/**".into(), "node_modules/**".into()];
    }
    for g in &exclude { gb.add(Glob::new(g).unwrap()); }
    let gset = gb.build().unwrap();

    let roots = if cfg.roots.is_empty() { vec![std::env::current_dir()?] } else { cfg.roots.clone() };

    // Write meta start
    let start_ts = now_rfc3339();
    {
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&out_path)?;
        writeln!(file, "{{\"type\":\"meta\",\"phase\":\"start\",\"ts\":\"{}\",\"roots\":{},\"exclude\":{}}}",
            start_ts,
            serde_json::to_string(&roots.iter().map(|p| p.display().to_string()).collect::<Vec<_>>())?,
            serde_json::to_string(&exclude)?)?;
    }

    let out_path_thread = out_path.clone();
    let join = thread::spawn(move || {
        // open writer inside thread
        let mut f = fs::OpenOptions::new().create(true).append(true).open(&out_path_thread).unwrap();
        let mut watcher = RecommendedWatcher::new(tx_ev, Config::default()).unwrap();
        for r in &roots {
            let _ = watcher.watch(r, notify::RecursiveMode::Recursive);
        }

        let mut lines: u64 = 1; // start meta
        let mut bytes: u64 = fs::metadata(&out_path_thread).ok().map(|m| m.len()).unwrap_or(0);
        let mut first_ts: Option<String> = None;
        let mut last_ts: Option<String>  = None;
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        loop {
            if let Ok(_) = rx_stop.try_recv() { break; }
            match rx_ev.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(evt)) => {
                    let ts = now_rfc3339();

                    // filter and relativize paths
                    let mut filtered: Vec<PathBuf> = vec![];
                    for p in &evt.paths {
                        let rp = match p.strip_prefix(&cwd) {
                            Ok(r) => r.to_path_buf(),
                            Err(_) => p.clone(),
                        };
                        if gset.is_match(&rp) { continue; }
                        filtered.push(rp);
                    }
                    if filtered.is_empty() { continue; }

                    let mut maybe_line: Option<String> = None;

                    match evt.kind {
                        EventKind::Create(CreateKind::Any)|EventKind::Create(CreateKind::File)|EventKind::Create(CreateKind::Folder) => {
                            let p = filtered[0].display().to_string();
                            let mut obj = serde_json::json!({
                                "schema": "file_watch_event.v0.1",
                                "op": "create",
                                "path": p,
                                "ts": ts
                            });
                            if let Ok(meta) = fs::metadata(filtered[0].clone()) {
                                obj["size"] = serde_json::json!(meta.len());
                            } else {
                                obj["size"] = serde_json::Value::Null;
                            }
                            if cfg.hash && filtered[0].is_file() {
                                obj["sha256"] = sha256_file(&filtered[0]).map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
                            } else {
                                obj["sha256"] = serde_json::Value::Null;
                            }
                            maybe_line = Some(serde_json::to_string(&obj).unwrap());
                        }
                        EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Any) => {
                            let p = filtered[0].display().to_string();
                            let mut obj = serde_json::json!({
                                "schema": "file_watch_event.v0.1",
                                "op": "modify",
                                "path": p,
                                "ts": ts
                            });
                            if let Ok(meta) = fs::metadata(filtered[0].clone()) {
                                obj["size"] = serde_json::json!(meta.len());
                            } else {
                                obj["size"] = serde_json::Value::Null;
                            }
                            if cfg.hash && filtered[0].is_file() {
                                obj["sha256"] = sha256_file(&filtered[0]).map(serde_json::Value::from).unwrap_or(serde_json::Value::Null);
                            } else {
                                obj["sha256"] = serde_json::Value::Null;
                            }
                            maybe_line = Some(serde_json::to_string(&obj).unwrap());
                        }
                        EventKind::Remove(RemoveKind::Any)|EventKind::Remove(RemoveKind::File)|EventKind::Remove(RemoveKind::Folder) => {
                            let p = filtered[0].display().to_string();
                            let obj = serde_json::json!({
                                "schema": "file_watch_event.v0.1",
                                "op": "delete",
                                "path": p,
                                "ts": ts
                            });
                            maybe_line = Some(serde_json::to_string(&obj).unwrap());
                        }
                        EventKind::Modify(ModifyKind::Name(RenameMode::Both))
                        | EventKind::Modify(ModifyKind::Name(RenameMode::From))
                        | EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                            let (from, to) = if filtered.len() >= 2 {
                                (filtered[0].display().to_string(), filtered[1].display().to_string())
                            } else {
                                let p = filtered[0].display().to_string();
                                (p.clone(), p)
                            };
                            let obj = serde_json::json!({
                                "schema": "file_watch_event.v0.1",
                                "op": "rename",
                                "from": from,
                                "to": to,
                                "ts": ts
                            });
                            maybe_line = Some(serde_json::to_string(&obj).unwrap());
                        }
                        _ => { /* ignore other noise */ }
                    }

                    if let Some(line) = maybe_line {
                        let _ = writeln!(f, "{}", line);
                        lines += 1;
                        bytes += (line.len() + 1) as u64;
                        last_ts = Some(ts.clone());
                        if first_ts.is_none() {
                            first_ts = Some(ts.clone());
                        }
                    }
                }
                _ => { /* timeout */ }
            }
        }

        let end_ts = now_rfc3339();
        let trailer = format!("{{\"type\":\"meta\",\"phase\":\"end\",\"ts\":\"{}\",\"total\":{}}}\n", end_ts, lines.saturating_sub(1));
        let _ = f.write_all(trailer.as_bytes());
        lines += 1;
        bytes += trailer.len() as u64;

        (lines, bytes, first_ts.unwrap_or_else(|| end_ts.clone()), last_ts.unwrap_or(end_ts))
    });

    Ok(FsTopicHandle{ path: out_path, join: Some(join), stop_tx: Some(tx_stop) })
}

fn fs_topic_stop(mut h: FsTopicHandle) -> anyhow::Result<(PathBuf, u64, u64, String, String, String)> {
    if let Some(tx) = h.stop_tx.take() { let _ = tx.send(()); }
    let (lines, bytes, from_ts, to_ts) = h.join.take().unwrap().join().unwrap();
    let sha = sha256_file(&h.path).unwrap_or_else(|| "sha256:0".into());
    Ok((h.path, lines, bytes, sha, from_ts, to_ts))
}
