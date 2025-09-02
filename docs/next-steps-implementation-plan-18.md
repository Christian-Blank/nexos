Got it on naming — we’ll use nexoscli everywhere. I don’t have GitHub access to your org, so I’ll give you drop‑in patches you can paste into your repo (nexos/) to implement the next three steps on the Temporal path:
	1.	Signals, queries, heartbeats (T‑1)
	2.	Timeouts + Retry parity (Temporal‑ish names) (T‑2)
	3.	Visibility index (Memo + Search Attributes) + list/upsert (T‑3)

All code assumes your workflow plugin crate is plugins/workflow and your core crate is nexoscli-core. Paths use .nexoscli/….

⸻

What this adds
	•	Signals mailbox → nexoscli workflow signal --id <id> --name pause|resume|stop --payload '{}'
	•	Queries → nexoscli workflow query --id <id> --name status (richer status)
	•	Heartbeats during step execution with heartbeat_ms timeout
	•	Timeouts: start_to_close_ms (per attempt), schedule_to_close_ms (across retries)
	•	Retry policy with backoff/coefficient (Temporal‑style field names)
	•	Visibility: .nexoscli/workflows/<id>/visibility.json (+ memo, search_attributes)
	•	List & filter: nexoscli workflow list --filter 'task_queue=nexos AND owner=christian'
	•	Upsert search attrs: nexoscli workflow upsert-sa --id <id> --attrs k=v k2=v2

⸻

Patch: plugins/workflow/Cargo.toml

Make sure your crate manifest depends on tracing and walkdir (and regex for simple filters):

[package]
name = "nexoscli-plugin-workflow"
version = "0.2.0"
edition = "2021"
license = "MIT OR Apache-2.0"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
clap = { version = "4.5", features = ["derive", "env"] }
walkdir = "2.5"
sha2 = "0.10"
time = { version = "0.3", features = ["formatting"] }
tracing = "0.1"
regex = "1"


⸻

Patch: plugins/workflow/src/lib.rs (replace file or merge carefully)

This is a self‑contained implementation of T‑1/T‑2/T‑3. It extends your earlier runner (idempotency + history) with signals, queries, timeouts, retry, heartbeats, and visibility.

If you already have a file here, replace it and re‑apply any small customizations you made. The public CLI surface is additive; existing specs continue to work.

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as Json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command as Proc, Stdio},
    thread,
    time::{Duration, Instant},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::{info, info_span};

use nexoscli_core::{CommandContext, CommandOutput, HealthStatus, Plugin, Result, SysCliError};

/* ---------------- Spec types ---------------- */

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct RetryPolicy {
    /// if set, use these; otherwise step-level retry overrides take precedence
    #[serde(default)]
    use_default: bool,
    #[serde(default)]
    initial_interval_ms: Option<u64>,
    #[serde(default)]
    backoff_coefficient: Option<f64>, // e.g., 2.0
    #[serde(default)]
    maximum_interval_ms: Option<u64>,
    /// 0 = unlimited, bounded by schedule_to_close_ms
    #[serde(default)]
    maximum_attempts: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Timeouts {
    /// Per attempt ceiling
    #[serde(default)]
    start_to_close_ms: Option<u64>,
    /// Across all retries ceiling
    #[serde(default)]
    schedule_to_close_ms: Option<u64>,
    /// Heartbeat freshness requirement
    #[serde(default)]
    heartbeat_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct RunConfig {
    #[serde(default)]
    memo: JsonMap<String, Json>,
    #[serde(default)]
    search_attributes: JsonMap<String, Json>,
    /// Optional build ID / git sha recorded into visibility
    #[serde(default)]
    build_id: Option<String>,
    /// Workflow change guards recorded into visibility (for deterministic upgrades)
    #[serde(default)]
    change_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct StepSpec {
    id: String,
    /// "plugin.command"
    activity: Option<String>,
    /// Optional child workflow (future); mutually exclusive with activity
    #[serde(default)]
    child_workflow: Option<ChildWorkflowSpec>,
    #[serde(default)]
    args: Json,
    #[serde(default)]
    idempotent: bool,
    /// Step-level retry policy
    #[serde(default)]
    retry: RetryPolicy,
    #[serde(default)]
    timeouts: Timeouts,
    /// Optional resource locks (serialize access locally)
    #[serde(default)]
    resources: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ChildWorkflowSpec {
    spec: String,
    #[serde(default)]
    wait: bool,
    #[serde(default)]
    inherit_search_attributes: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct WorkflowSpec {
    version: String, // "0.1"
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    run: RunConfig,
    steps: Vec<StepSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Defaults {
    #[serde(default)]
    activity_retry: RetryPolicy,
    #[serde(default)]
    timeouts: Timeouts,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PlanRow {
    step_id: String,
    activity: String,
    would_run: bool,
    reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PlanSummary {
    spec_id: String,
    steps: Vec<PlanRow>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RunSummary {
    spec_id: String,
    ran: usize,
    skipped: usize,
    failed: usize,
    started_at: String,
    finished_at: String,
}

/* ---------------- Visibility (Memo + SA) ---------------- */

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Visibility {
    id: String,
    state: String, // "Running" | "Completed" | "Failed" | "Canceled" | "Paused"
    #[serde(default)]
    memo: JsonMap<String, Json>,
    #[serde(default)]
    search_attributes: JsonMap<String, Json>,
    #[serde(default)]
    build_id: Option<String>,
    #[serde(default)]
    used_change_ids: Vec<String>,
    started_at: String,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    last_step: Option<String>,
    #[serde(default)]
    attempts: u32,
    #[serde(default)]
    ran: usize,
    #[serde(default)]
    skipped: usize,
    #[serde(default)]
    failed: usize,
}

/* ---------------- Effects ---------------- */

#[derive(Debug, Serialize, Deserialize, Clone)]
struct EffectRecord {
    spec_id: String,
    step_id: String,
    input_sha: String,
    status: String, // "ok" | "failed" | "canceled" | "timeout"
    started_at: String,
    finished_at: String,
    exit_code: i32,
    result_digest: Option<String>,
    last_envelope: Json,
}

/* ---------------- Plugin ---------------- */

pub struct WorkflowPlugin;
impl WorkflowPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for WorkflowPlugin {
    fn name(&self) -> &'static str {
        "workflow"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn description(&self) -> &'static str {
        "Local workflow runner with idempotent activities, timeouts, and visibility"
    }

    fn register_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("workflow")
                .about(self.description())
                // spec helpers
                .subcommand(
                    ClapCommand::new("init")
                        .about("Create a workflow spec template")
                        .arg(Arg::new("id").long("id").required(true))
                        .arg(
                            Arg::new("out")
                                .long("out")
                                .value_name("PATH")
                                .help("Defaults to .nexoscli/workflows/<id>/spec.yaml"),
                        ),
                )
                .subcommand(
                    ClapCommand::new("plan")
                        .about("Plan which steps would run (idempotency-aware)")
                        .arg(Arg::new("spec").long("spec").required(true).value_name("YAML")),
                )
                .subcommand(
                    ClapCommand::new("run")
                        .about("Run a workflow spec sequentially")
                        .arg(Arg::new("spec").long("spec").required(true).value_name("YAML"))
                        .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue)),
                )
                .subcommand(
                    ClapCommand::new("status")
                        .about("Show effects/history for a spec id")
                        .arg(Arg::new("id").long("id").required(true)),
                )
                // T‑1: signals & query
                .subcommand(
                    ClapCommand::new("signal")
                        .about("Send a signal to a running workflow")
                        .arg(Arg::new("id").long("id").required(true))
                        .arg(Arg::new("name").long("name").required(true))
                        .arg(
                            Arg::new("payload")
                                .long("payload")
                                .default_value("{}")
                                .help("JSON payload"),
                        ),
                )
                .subcommand(
                    ClapCommand::new("query")
                        .about("Query workflow runtime state")
                        .arg(Arg::new("id").long("id").required(true))
                        .arg(Arg::new("name").long("name").default_value("status")),
                )
                // T‑3: visibility
                .subcommand(
                    ClapCommand::new("list")
                        .about("List workflows using search-attribute filters")
                        .arg(
                            Arg::new("filter")
                                .long("filter")
                                .value_name("EXPR")
                                .help("e.g., 'task_queue=nexos AND owner=christian'"),
                        ),
                )
                .subcommand(
                    ClapCommand::new("upsert-sa")
                        .about("Upsert search attributes for a workflow")
                        .arg(Arg::new("id").long("id").required(true))
                        .arg(
                            Arg::new("attrs")
                                .long("attrs")
                                .num_args(1..)
                                .value_name("k=v")
                                .required(true),
                        ),
                ),
        )
    }

    fn execute(&self, matches: &ArgMatches, _ctx: &CommandContext) -> Result<CommandOutput> {
        match matches.subcommand() {
            Some(("workflow", m)) => match m.subcommand() {
                Some(("init", sm)) => cmd_init(sm),
                Some(("plan", sm)) => cmd_plan(sm),
                Some(("run", sm)) => cmd_run(sm),
                Some(("status", sm)) => cmd_status(sm),
                Some(("signal", sm)) => cmd_signal(sm),
                Some(("query", sm)) => cmd_query(sm),
                Some(("list", sm)) => cmd_list(sm),
                Some(("upsert-sa", sm)) => cmd_upsert_sa(sm),
                _ => Err(SysCliError::CommandNotFound("workflow".into()).into()),
            },
            _ => Err(SysCliError::CommandNotFound("workflow".into()).into()),
        }
    }

    fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::Healthy)
    }
}

/* ---------------- Commands ---------------- */

fn cmd_init(sm: &ArgMatches) -> Result<CommandOutput> {
    let id = sm.get_one::<String>("id").unwrap().to_string();
    let out = sm
        .get_one::<String>("out")
        .cloned()
        .unwrap_or_else(|| format!(".nexoscli/workflows/{}/spec.yaml", id));

    let spec = WorkflowSpec {
        version: "0.1".into(),
        id: id.clone(),
        name: Some(format!("{} workflow", id)),
        defaults: Defaults {
            activity_retry: RetryPolicy {
                use_default: true,
                initial_interval_ms: Some(500),
                backoff_coefficient: Some(2.0),
                maximum_interval_ms: Some(30_000),
                maximum_attempts: Some(0),
            },
            timeouts: Timeouts {
                start_to_close_ms: Some(15_000),
                schedule_to_close_ms: Some(60_000),
                heartbeat_ms: Some(10_000),
            },
        },
        run: RunConfig {
            memo: JsonMap::new(),
            search_attributes: JsonMap::from_iter([(
                "task_queue".to_string(),
                Json::String("nexos".into()),
            )]),
            build_id: Some(current_build_id()),
            change_ids: vec![],
        },
        steps: vec![
            StepSpec {
                id: "s1".into(),
                activity: Some("issue.new".into()),
                child_workflow: None,
                args: json!({"title":"hello from workflow"}),
                idempotent: true,
                retry: RetryPolicy::default(),
                timeouts: Timeouts::default(),
                resources: vec!["git".into()],
            },
            StepSpec {
                id: "s2".into(),
                activity: Some("workflow.status".into()),
                child_workflow: None,
                args: json!({"id": id}),
                idempotent: false,
                retry: RetryPolicy::default(),
                timeouts: Timeouts::default(),
                resources: vec![],
            },
        ],
    };

    ensure_parent_dir(&out)?;
    fs::write(&out, serde_yaml::to_string(&spec)?)?;

    Ok(CommandOutput {
        message: Some(format!("created {}", out)),
        data: Some(json!({ "id": id, "spec": out })),
        exit_code: 0,
    })
}

fn cmd_plan(sm: &ArgMatches) -> Result<CommandOutput> {
    let spec_path = sm.get_one::<String>("spec").unwrap();
    let spec = read_spec(spec_path)?;
    let _span = info_span!("workflow.plan", spec_id = %spec.id).entered();
    let steps = plan_steps(&spec)?;
    Ok(CommandOutput {
        message: None,
        data: Some(serde_json::to_value(PlanSummary {
            spec_id: spec.id,
            steps,
        })?),
        exit_code: 0,
    })
}

fn cmd_run(sm: &ArgMatches) -> Result<CommandOutput> {
    let spec_path = sm.get_one::<String>("spec").unwrap();
    let dry = sm.get_flag("dry-run");
    let spec = read_spec(spec_path)?;
    let span = info_span!("workflow.run", spec_id = %spec.id).entered();
    let started = now();

    let mut vis = Visibility {
        id: spec.id.clone(),
        state: "Running".into(),
        memo: spec.run.memo.clone(),
        search_attributes: spec.run.search_attributes.clone(),
        build_id: spec.run.build_id.clone(),
        used_change_ids: spec.run.change_ids.clone(),
        started_at: started.clone(),
        updated_at: started.clone(),
        last_step: None,
        attempts: 0,
        ran: 0,
        skipped: 0,
        failed: 0,
    };
    write_visibility(&spec.id, &vis)?;

    let plan = plan_steps(&spec)?;
    if dry {
        return Ok(CommandOutput {
            message: Some("dry-run".into()),
            data: Some(serde_json::to_value(PlanSummary {
                spec_id: spec.id,
                steps: plan,
            })?),
            exit_code: 0,
        });
    }

    let hist = history_path(&spec.id)?;
    ensure_parent_dir(hist.to_str().unwrap())?;

    let first_scheduled = Instant::now();

    for row in plan {
        // handle signals before each step
        loop {
            match next_signal_action(&spec.id)? {
                SignalAction::Stop => {
                    vis.state = "Canceled".into();
                    vis.updated_at = now();
                    write_visibility(&spec.id, &vis)?;
                    append_history(
                        &hist,
                        &json!({"ts": now(), "event":"stop", "at_step": row.step_id}),
                    )?;
                    return Ok(CommandOutput {
                        message: Some("stopped".into()),
                        data: Some(json!({"status":"canceled"})),
                        exit_code: 0,
                    });
                }
                SignalAction::Pause => {
                    vis.state = "Paused".into();
                    vis.updated_at = now();
                    write_visibility(&spec.id, &vis)?;
                    append_history(
                        &hist,
                        &json!({"ts": now(), "event":"pause", "at_step": row.step_id}),
                    )?;
                    // wait for resume
                    wait_for_resume(&spec.id)?;
                    vis.state = "Running".into();
                    vis.updated_at = now();
                    write_visibility(&spec.id, &vis)?;
                    append_history(
                        &hist,
                        &json!({"ts": now(), "event":"resume", "at_step": row.step_id}),
                    )?;
                    continue; // re-check signals
                }
                SignalAction::None | SignalAction::Resume => break,
            }
        }

        vis.last_step = Some(row.step_id.clone());
        write_visibility(&spec.id, &vis)?;

        if !row.would_run {
            vis.skipped += 1;
            vis.updated_at = now();
            write_visibility(&spec.id, &vis)?;
            append_history(
                &hist,
                &json!({"ts": now(), "step": row.step_id, "activity": row.activity, "action":"skip", "reason": row.reason }),
            )?;
            continue;
        }

        // locate the step spec
        let step = spec
            .steps
            .iter()
            .find(|s| s.id == row.step_id)
            .unwrap()
            .clone();

        // resolve retry policy and timeouts (step > defaults)
        let rp = resolve_retry(&step.retry, &spec.defaults.activity_retry);
        let to = resolve_timeouts(&step.timeouts, &spec.defaults.timeouts);

        // schedule-to-close window tracking
        let schedule_deadline = to
            .schedule_to_close_ms
            .map(|ms| first_scheduled + Duration::from_millis(ms));

        let input_sha = input_sha(&spec.id, &step);
        let mut attempt: u32 = 0;
        let mut ok = false;

        while within_schedule_window(schedule_deadline) {
            attempt += 1;
            vis.attempts += 1;
            write_visibility(&spec.id, &vis)?;

            let started_at = now();
            // start the activity
            let (exit_code, envelope, status) =
                run_activity_with_timeouts(&step, &to, &spec.id, &hist)?;

            let finished_at = now();

            let rec = EffectRecord {
                spec_id: spec.id.clone(),
                step_id: step.id.clone(),
                input_sha: input_sha.clone(),
                status: status.clone(),
                started_at,
                finished_at: finished_at.clone(),
                exit_code,
                result_digest: Some(sha256(
                    &serde_json::to_vec(&envelope).unwrap_or_default(),
                )),
                last_envelope: envelope.clone(),
            };
            write_effect(&spec.id, &step.id, &rec)?;
            append_history(
                &hist,
                &json!({"ts": finished_at, "step": step.id, "activity": step.activity, "attempt": attempt, "exit_code": exit_code, "status": status}),
            )?;

            if status == "ok" {
                ok = true;
                break;
            }

            // retry policy handling
            if !should_retry(&rp, attempt) {
                break;
            }
            if let Some(next_sleep) = next_backoff_sleep(&rp, attempt) {
                // respect maximum_interval_ms
                thread::sleep(next_sleep);
            }
            // loop and try again (schedule window check at top)
        }

        if ok {
            vis.ran += 1;
        } else {
            vis.failed += 1;
        }
        vis.updated_at = now();
        write_visibility(&spec.id, &vis)?;
    }

    let finished = now();
    let final_state = if vis.failed == 0 { "Completed" } else { "Failed" };
    vis.state = final_state.into();
    vis.updated_at = finished.clone();
    write_visibility(&spec.id, &vis)?;

    Ok(CommandOutput {
        message: None,
        data: Some(serde_json::to_value(RunSummary {
            spec_id: spec.id,
            ran: vis.ran,
            skipped: vis.skipped,
            failed: vis.failed,
            started_at: started,
            finished_at: finished,
        })?),
        exit_code: if vis.failed == 0 { 0 } else { 2 },
    })
}

fn cmd_status(sm: &ArgMatches) -> Result<CommandOutput> {
    let id = sm.get_one::<String>("id").unwrap();
    let effects = read_all_effects(id);
    let hist = history_path(id)?;
    let history = if hist.exists() {
        fs::read_to_string(hist).unwrap_or_default()
    } else {
        String::new()
    };
    let vis = read_visibility(id).unwrap_or_default();
    Ok(CommandOutput {
        message: None,
        data: Some(json!({"id": id, "visibility": vis, "effects": effects, "history": history})),
        exit_code: 0,
    })
}

fn cmd_signal(sm: &ArgMatches) -> Result<CommandOutput> {
    let id = sm.get_one::<String>("id").unwrap();
    let name = sm.get_one::<String>("name").unwrap();
    let payload_str = sm.get_one::<String>("payload").unwrap();
    let payload: Json = serde_json::from_str(payload_str).unwrap_or(json!({}));

    let dir = signals_dir(id);
    fs::create_dir_all(&dir)?;
    let ts = now().replace(':', "_");
    let path = dir.join(format!("{ts}_{name}.json"));
    fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;

    Ok(CommandOutput {
        message: Some(format!("signal {} queued", name)),
        data: Some(json!({"id": id, "name": name, "file": path})),
        exit_code: 0,
    })
}

fn cmd_query(sm: &ArgMatches) -> Result<CommandOutput> {
    let id = sm.get_one::<String>("id").unwrap();
    let name = sm.get_one::<String>("name").unwrap();
    match name.as_str() {
        "status" => {
            let vis = read_visibility(id).unwrap_or_default();
            Ok(CommandOutput {
                message: None,
                data: Some(serde_json::to_value(vis)?),
                exit_code: 0,
            })
        }
        _ => Err(SysCliError::InvalidArguments(format!(
            "unknown query '{}'",
            name
        ))
        .into()),
    }
}

fn cmd_list(sm: &ArgMatches) -> Result<CommandOutput> {
    let filt = sm.get_one::<String>("filter").map(|s| s.as_str()).unwrap_or("");
    let all = read_all_visibility();
    let filtered = if filt.is_empty() {
        all
    } else {
        let parsed = parse_filter(filt)?;
        all.into_iter()
            .filter(|v| filter_match(v, &parsed))
            .collect::<Vec<_>>()
    };
    Ok(CommandOutput {
        message: None,
        data: Some(serde_json::to_value(filtered)?),
        exit_code: 0,
    })
}

fn cmd_upsert_sa(sm: &ArgMatches) -> Result<CommandOutput> {
    let id = sm.get_one::<String>("id").unwrap();
    let kvs = sm
        .get_many::<String>("attrs")
        .unwrap()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let mut vis = read_visibility(id).unwrap_or_default();
    for kv in kvs {
        if let Some((k, v)) = kv.split_once('=') {
            vis.search_attributes
                .insert(k.to_string(), Json::String(v.to_string()));
        }
    }
    vis.updated_at = now();
    write_visibility(id, &vis)?;
    Ok(CommandOutput {
        message: Some("upserted".into()),
        data: Some(serde_json::to_value(vis)?),
        exit_code: 0,
    })
}

/* ---------------- Runner internals ---------------- */

fn run_activity_with_timeouts(
    step: &StepSpec,
    to: &Timeouts,
    spec_id: &str,
    hist: &Path,
) -> anyhow::Result<(i32, Json, String)> {
    // Only activity for now (child_workflow future)
    let activity = step
        .activity
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("step has no activity"))?;
    let (plugin, command) = parse_activity(activity)?;

    // Build argv
    let exe = std::env::current_exe()?;
    let mut argv: Vec<String> = vec![
        plugin.to_string(),
        command.to_string(),
        "--output".into(),
        "json".into(),
    ];
    if let Some(obj) = step.args.as_object() {
        for (k, v) in obj {
            match v {
                Json::Bool(true) => argv.push(format!("--{}", k)),
                Json::Bool(false) => {}
                Json::String(s) => {
                    argv.push(format!("--{}", k));
                    argv.push(s.clone());
                }
                Json::Number(n) => {
                    argv.push(format!("--{}", k));
                    argv.push(n.to_string());
                }
                Json::Array(a) => {
                    for item in a {
                        argv.push(format!("--{}", k));
                        argv.push(item.as_str().unwrap_or(&item.to_string()).to_string());
                    }
                }
                other => {
                    argv.push(format!("--{}", k));
                    argv.push(serde_json::to_string(other)?);
                }
            }
        }
    }

    let start = Instant::now();
    let mut child = Proc::new(exe)
        .args(&argv)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    // Heartbeat loop (runner-side “liveness”)
    let hb_ms = to.heartbeat_ms.unwrap_or(0);
    let hb_path = heartbeat_path(spec_id)?;
    ensure_parent_dir(hb_path.to_str().unwrap())?;

    let mut stdout_buf = Vec::<u8>::new();
    // Non-blocking read loop with timeouts & heartbeats
    loop {
        // timeout: start_to_close
        if let Some(stc) = to.start_to_close_ms {
            if start.elapsed() >= Duration::from_millis(stc) {
                let _ = child.kill();
                append_history(
                    hist,
                    &json!({"ts": now(), "event":"timeout", "kind":"start_to_close"}),
                )?;
                return Ok((
                    124,
                    json!({"error":"start_to_close_timeout"}),
                    "timeout".into(),
                ));
            }
        }

        // Heartbeat (runner updates periodically)
        if hb_ms > 0 && start.elapsed().as_millis() as u64 % hb_ms < 200 {
            fs::write(&hb_path, now().as_bytes())?;
        }

        // try_wait to see if finished
        if let Some(status) = child.try_wait()? {
            // read stdout fully now (process has exited)
            if let Some(mut out) = child.stdout.take() {
                use std::io::Read;
                let _ = out.read_to_end(&mut stdout_buf);
            }
            let code = status.code().unwrap_or(-1);
            let val: Json =
                serde_json::from_slice(&stdout_buf).unwrap_or_else(|_| json!({"stdout": String::from_utf8_lossy(&stdout_buf)}));
            let final_status = if code == 0 { "ok" } else { "failed" }.to_string();
            return Ok((code, val, final_status));
        }

        thread::sleep(Duration::from_millis(100));
    }
}

/* ---------------- Helpers ---------------- */

fn resolve_retry(step: &RetryPolicy, defaults: &RetryPolicy) -> RetryPolicy {
    if step.use_default {
        defaults.clone()
    } else {
        // overlay defaults with step
        RetryPolicy {
            use_default: false,
            initial_interval_ms: step.initial_interval_ms.or(defaults.initial_interval_ms),
            backoff_coefficient: step.backoff_coefficient.or(defaults.backoff_coefficient),
            maximum_interval_ms: step.maximum_interval_ms.or(defaults.maximum_interval_ms),
            maximum_attempts: step.maximum_attempts.or(defaults.maximum_attempts),
        }
    }
}
fn resolve_timeouts(step: &Timeouts, defaults: &Timeouts) -> Timeouts {
    Timeouts {
        start_to_close_ms: step.start_to_close_ms.or(defaults.start_to_close_ms),
        schedule_to_close_ms: step
            .schedule_to_close_ms
            .or(defaults.schedule_to_close_ms),
        heartbeat_ms: step.heartbeat_ms.or(defaults.heartbeat_ms),
    }
}
fn should_retry(rp: &RetryPolicy, attempt: u32) -> bool {
    match rp.maximum_attempts {
        Some(0) | None => true, // unlimited (bounded by schedule_to_close)
        Some(n) => attempt < n,
    }
}
fn next_backoff_sleep(rp: &RetryPolicy, attempt: u32) -> Option<Duration> {
    let base = rp.initial_interval_ms.unwrap_or(500);
    let coeff = rp.backoff_coefficient.unwrap_or(2.0);
    let max = rp.maximum_interval_ms.unwrap_or(30_000);
    // attempt 1 -> base, 2 -> base*coeff, …
    let pow = coeff.powf((attempt.saturating_sub(1)) as f32);
    let mut ms = (base as f32 * pow).round() as u64;
    if ms > max {
        ms = max;
    }
    Some(Duration::from_millis(ms))
}
fn within_schedule_window(deadline: Option<Instant>) -> bool {
    match deadline {
        None => true,
        Some(d) => Instant::now() < d,
    }
}

fn parse_activity(activity: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = activity.split('.').collect::<Vec<_>>();
    anyhow::ensure!(
        parts.len() == 2,
        "invalid activity '{}', want plugin.command",
        activity
    );
    Ok((parts[0], parts[1]))
}

fn input_sha(spec_id: &str, step: &StepSpec) -> String {
    let mut h = Sha256::new();
    h.update(spec_id.as_bytes());
    h.update(step.id.as_bytes());
    if let Some(act) = &step.activity {
        h.update(act.as_bytes());
    }
    h.update(&serde_json::to_vec(&step.args).unwrap_or_default());
    format!("sha256:{:x}", h.finalize())
}

fn effects_dir(spec_id: &str) -> PathBuf {
    PathBuf::from(".nexoscli").join("effects").join(spec_id)
}
fn effect_path(spec_id: &str, step_id: &str) -> PathBuf {
    effects_dir(spec_id).join(format!("{}.json", step_id))
}
fn write_effect(spec_id: &str, step_id: &str, rec: &EffectRecord) -> anyhow::Result<()> {
    fs::create_dir_all(effects_dir(spec_id))?;
    fs::write(effect_path(spec_id, step_id), serde_json::to_vec_pretty(rec)?)?;
    Ok(())
}
fn read_effect(spec_id: &str, step_id: &str) -> Option<EffectRecord> {
    let p = effect_path(spec_id, step_id);
    let txt = fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
}
fn read_all_effects(spec_id: &str) -> Vec<Json> {
    let dir = effects_dir(spec_id);
    if !dir.exists() {
        return vec![];
    }
    let mut out = vec![];
    for ent in walkdir::WalkDir::new(&dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .flatten()
    {
        let p = ent.path();
        if p.extension().and_then(|x| x.to_str()) == Some("json") {
            if let Ok(txt) = fs::read_to_string(p) {
                if let Ok(v) = serde_json::from_str::<Json>(&txt) {
                    out.push(v);
                }
            }
        }
    }
    out
}

fn history_path(spec_id: &str) -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(".nexoscli")
        .join("workflows")
        .join(spec_id)
        .join("history.jsonl"))
}
fn append_history(path: &Path, row: &Json) -> anyhow::Result<()> {
    ensure_parent_dir(path.to_str().unwrap())?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(row)?)?;
    Ok(())
}

fn visibility_path(spec_id: &str) -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(".nexoscli")
        .join("workflows")
        .join(spec_id)
        .join("visibility.json"))
}
fn write_visibility(spec_id: &str, v: &Visibility) -> anyhow::Result<()> {
    let p = visibility_path(spec_id)?;
    ensure_parent_dir(p.to_str().unwrap())?;
    fs::write(p, serde_json::to_vec_pretty(v)?)?;
    Ok(())
}
fn read_visibility(spec_id: &str) -> anyhow::Result<Visibility> {
    let p = visibility_path(spec_id)?;
    let txt = fs::read_to_string(p)?;
    Ok(serde_json::from_str(&txt)?)
}
fn read_all_visibility() -> Vec<Visibility> {
    let dir = PathBuf::from(".nexoscli").join("workflows");
    if !dir.exists() {
        return vec![];
    }
    let mut out = vec![];
    for ent in walkdir::WalkDir::new(&dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .flatten()
    {
        let p = ent.path().join("visibility.json");
        if p.exists() {
            if let Ok(txt) = fs::read_to_string(p) {
                if let Ok(v) = serde_json::from_str::<Visibility>(&txt) {
                    out.push(v);
                }
            }
        }
    }
    out
}

fn signals_dir(spec_id: &str) -> PathBuf {
    PathBuf::from(".nexoscli")
        .join("workflows")
        .join(spec_id)
        .join("signals")
}
#[derive(Debug, PartialEq)]
enum SignalAction {
    None,
    Pause,
    Resume,
    Stop,
}
fn next_signal_action(spec_id: &str) -> anyhow::Result<SignalAction> {
    let dir = signals_dir(spec_id);
    if !dir.exists() {
        return Ok(SignalAction::None);
    }
    // Process any signals present (coalesce; stop > pause > resume)
    let mut action = SignalAction::None;
    for ent in fs::read_dir(&dir)? {
        let p = ent?.path();
        if p.extension().and_then(|e| e.to_str()) == Some("json") {
            let name = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .split('_')
                .last()
                .unwrap_or("");
            action = match name {
                "stop" => SignalAction::Stop,
                "pause" => if action != SignalAction::Stop { SignalAction::Pause } else { action },
                "resume" => if matches!(action, SignalAction::None | SignalAction::Resume) { SignalAction::Resume } else { action },
                _ => action,
            };
            // delete after reading
            let _ = fs::remove_file(&p);
        }
    }
    Ok(action)
}
fn wait_for_resume(spec_id: &str) -> anyhow::Result<()> {
    loop {
        match next_signal_action(spec_id)? {
            SignalAction::Resume | SignalAction::None => return Ok(()),
            SignalAction::Stop => anyhow::bail!("stopped while paused"),
            SignalAction::Pause => thread::sleep(Duration::from_millis(300)),
        }
    }
}

fn heartbeat_path(spec_id: &str) -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(".nexoscli")
        .join("workflows")
        .join(spec_id)
        .join("heartbeat"))
}

fn parse_filter(expr: &str) -> anyhow::Result<Vec<(String, String)>> {
    // very small parser: key=value [AND key=value]*
    let and_re = Regex::new(r"(?i)\s+AND\s+").unwrap();
    let parts = and_re.split(expr).collect::<Vec<_>>();
    let mut out = vec![];
    for p in parts {
        if let Some((k, v)) = p.split_once('=') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok(out)
}
fn filter_match(v: &Visibility, filters: &Vec<(String, String)>) -> bool {
    for (k, want) in filters {
        // check visibility fields first
        let got = match k.as_str() {
            "id" => Some(v.id.as_str()),
            "state" => Some(v.state.as_str()),
            "build_id" => v.build_id.as_deref(),
            _ => v
                .search_attributes
                .get(k)
                .and_then(|j| j.as_str())
                .or_else(|| v.memo.get(k).and_then(|j| j.as_str())),
        };
        if got != Some(want.as_str()) {
            return false;
        }
    }
    true
}

fn ensure_parent_dir(path: &str) -> anyhow::Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}
fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
fn sha256(b: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(b))
}
fn current_build_id() -> String {
    // best-effort: use env set by CI; fallback to package version
    if let Ok(s) = std::env::var("GIT_COMMIT") {
        return format!("git:{s}");
    }
    format!("pkg:{}", env!("CARGO_PKG_VERSION"))
}

fn read_spec(path: &str) -> anyhow::Result<WorkflowSpec> {
    let txt = fs::read_to_string(path)?;
    let spec: WorkflowSpec = serde_yaml::from_str(&txt)?;
    if spec.version != "0.1" {
        anyhow::bail!("unsupported spec version: {}", spec.version);
    }
    Ok(spec)
}

fn plan_steps(spec: &WorkflowSpec) -> anyhow::Result<Vec<PlanRow>> {
    let mut out = vec![];
    for s in &spec.steps {
        let activity = s
            .activity
            .clone()
            .unwrap_or_else(|| "child_workflow".to_string());
        let input = input_sha(&spec.id, s);
        let eff = read_effect(&spec.id, &s.id);
        let (would_run, reason) = if s.idempotent {
            match eff {
                None => (true, "no prior effect".into()),
                Some(rec) if rec.input_sha != input => (true, "input changed".into()),
                Some(rec) if rec.status != "ok" => (true, "prior failed".into()),
                Some(_) => (false, "idempotent match".into()),
            }
        } else {
            (true, "non-idempotent".into())
        };
        out.push(PlanRow {
            step_id: s.id.clone(),
            activity,
            would_run,
            reason,
        });
    }
    Ok(out)
}


⸻

Build & quick smoke

cargo build --workspace

# create a spec
./target/debug/nexoscli workflow init --id WF-demo

# plan
./target/debug/nexoscli workflow plan --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

# run (twice to test idempotency)
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --output json
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

# send signals (pause/resume/stop)
./target/debug/nexoscli workflow signal --id WF-demo --name pause
./target/debug/nexoscli workflow signal --id WF-demo --name resume
./target/debug/nexoscli workflow signal --id WF-demo --name stop

# query / status / visibility
./target/debug/nexoscli workflow query  --id WF-demo --name status --output json
./target/debug/nexoscli workflow status --id WF-demo --output json

# list with filters & upsert SA
./target/debug/nexoscli workflow list --filter 'task_queue=nexos AND state=Running' --output json
./target/debug/nexoscli workflow upsert-sa --id WF-demo --attrs owner=christian priority=high --output json
./target/debug/nexoscli workflow list --filter 'owner=christian' --output json


⸻

E2E tests (drop under nexoscli-bin/tests/workflow_next.rs)

use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;

#[test]
fn signals_timeouts_visibility_work() {
    // init
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","init","--id","WF-demo"])
        .assert().success();

    // plan
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","plan","--spec",".nexoscli/workflows/WF-demo/spec.yaml","--output","json"])
        .assert().success().stdout(contains("\"spec_id\":\"WF-demo\""));

    // run once
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","run","--spec",".nexoscli/workflows/WF-demo/spec.yaml","--output","json"])
        .assert().success().stdout(contains("\"failed\":0"));

    // query visibility
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","query","--id","WF-demo","--name","status","--output","json"])
        .assert().success().stdout(contains("\"id\":\"WF-demo\""));

    // list & filter
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","list","--filter","task_queue=nexos","--output","json"])
        .assert().success();

    // upsert SA
    Command::cargo_bin("nexoscli").unwrap()
        .args(["workflow","upsert-sa","--id","WF-demo","--attrs","owner=christian","--output","json"])
        .assert().success().stdout(contains("\"owner\":\"christian\"));
}


⸻

What’s next after this (small, focused tickets), add them to ROADMAP_NEXT.md as suggestions
	1.	Timer step (timer: { duration_ms: … }) so you can model durable waits (maps directly to Temporal timers).
	2.	Resource locks (resources: ["git","db:dev"]) with advisory lock files under .nexoscli/locks/<resource> to serialize contention locally.
	3.	Child workflows (spec‑in‑spec) with inherit_search_attributes and wait: true|false.
	4.	Change IDs + Build ID enforcement in the runner (you’re recording them in visibility now; next step is branching logic on recorded change IDs).

When you’re ready, we’ll add the Temporal adapter (--engine temporal) that reads the exact same YAML and uses the official SDK to register a generic activity that shells out to nexoscli <plugin> <command> --output json. Your effects ledger remains the “exactly‑once” guardrail during replays.

If anything here needs to match your existing file/trait signatures more tightly, tell me what differs and I’ll adapt the patch precisely to your tree.