use clap::{ArgMatches, Args, Command, FromArgMatches};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::thread;
use std::time::Duration;

/// Args for `term run` using dual-derive and typed parsing.
/// Notes:
/// - `command` is captured as trailing var-args (Vec<String>) to preserve current multi-word behavior,
///   and then joined via super::sh_join for execution.
/// - `fs_root` and `fs_exclude` accept a single string which can be CSV or whitespace-separated; we normalize to Vec<PathBuf>/Vec<String>.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::Args)]
pub struct TermRunArgs {
    #[arg(long)]
    pub cwd: Option<String>,
    #[arg(long = "fs-exclude")]
    pub fs_exclude: Option<String>,
    #[arg(long = "fs-hash")]
    pub fs_hash: bool,
    #[arg(long = "fs-root")]
    pub fs_root: Option<String>,
    #[arg(long = "fs-topic")]
    pub fs_topic: bool,
    #[arg(long)]
    pub passthrough: bool,
    #[arg(long = "sample-kb")]
    pub sample_kb: Option<usize>,
    /// Trailing positional args forming the command to run (joined for shell)
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TermRunResult {
    pub cmd: String,
    pub cwd: String,
    pub exit_code: i32,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub stdout_sample: String,
    pub stderr_sample: String,
}

pub fn subcommand() -> Command {
    TermRunArgs::augment_args(Command::new("run").about("Execute a command and capture output"))
}

fn split_list(s: &Option<String>) -> Vec<String> {
    match s {
        None => vec![],
        Some(v) => v
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect(),
    }
}

fn normalize_roots(s: &Option<String>) -> Vec<PathBuf> {
    split_list(s).into_iter().map(PathBuf::from).collect()
}

pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let args = TermRunArgs::from_arg_matches(m).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let cwd = match args.cwd.clone() {
        Some(c) => c,
        None => std::env::current_dir()?.to_string_lossy().to_string(),
    };
    let sample = args.sample_kb.unwrap_or(64);
    let passthrough = args.passthrough;

    // FS topic normalization
    let fs_topic = args.fs_topic;
    let roots: Vec<PathBuf> = normalize_roots(&args.fs_root);
    let excludes: Vec<String> = split_list(&args.fs_exclude);
    let do_hash = args.fs_hash;

    // Start watcher (best-effort) before spawning child
    let mut fs_handle: Option<super::FsTopicHandle> = None;
    if fs_topic {
        fs_handle = Some(super::fs_topic_start(super::FsTopicCfg {
            roots,
            excludes,
            hash: do_hash,
        })?);
        // allow watcher to arm before running the command
        thread::sleep(Duration::from_millis(100));
    }

    // Join trailing args for shell
    let joined = super::sh_join(&args.command);

    // Shell for portability
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

    let (stdout_sample, stdout_sha256) = super::digest_and_sample(&output.stdout, sample);
    let (stderr_sample, stderr_sha256) = super::digest_and_sample(&output.stderr, sample);

    if passthrough {
        use std::io::Write;
        let _ = std::io::stdout().write_all(&output.stdout);
        let _ = std::io::stderr().write_all(&output.stderr);
    }

    let exit_code = output.status.code().unwrap_or(-1);

    // Stop watcher and emit fs.topic event if enabled
    if let Some(h) = fs_handle {
        // allow any in-flight events to flush before stopping
        thread::sleep(Duration::from_millis(200));
        let (path, lines, bytes, sha, from_ts, to_ts) = super::fs_topic_stop(h)?;
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
            0,
            None,
            None,
            None,
        );
    }

    let data = TermRunResult {
        cmd: joined.clone(),
        cwd: cwd.clone(),
        exit_code,
        stdout_sha256,
        stderr_sha256,
        stdout_sample,
        stderr_sample,
    };
    let _ = ctx.ok("term", "run", &data);
    // Return child's exit code as plugin exit code (preserving existing behavior)
    Ok(exit_code)
}
