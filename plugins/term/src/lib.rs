use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::{plugin::{Plugin, CommandContext}, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::process::Command as StdCommand;

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
