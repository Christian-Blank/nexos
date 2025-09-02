use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{
    plugin::{CommandContext, Plugin, ToolSpec},
    Result,
};
use regex::Regex;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as Json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::{Command as Proc, Stdio},
    thread,
    time::{Duration, Instant},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/* ---------------- Spec & Model Types ---------------- */

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
struct RetryPolicy {
    #[serde(default)]
    use_default: bool,
    #[serde(default)]
    initial_interval_ms: Option<u64>,
    #[serde(default)]
    backoff_coefficient: Option<f64>,
    #[serde(default)]
    maximum_interval_ms: Option<u64>,
    /// 0 = unlimited (bounded by schedule_to_close_ms)
    #[serde(default)]
    maximum_attempts: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
struct SimpleRetry {
    #[serde(default)]
    max_attempts: u32,
    #[serde(default)]
    backoff_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
enum RetryUnion {
    Simple(SimpleRetry),
    Temporal(RetryPolicy),
}
impl Default for RetryUnion {
    fn default() -> Self {
        RetryUnion::Simple(SimpleRetry::default())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
struct Timeouts {
    /// Per-attempt ceiling
    #[serde(default)]
    start_to_close_ms: Option<u64>,
    /// Across all retries for the step
    #[serde(default)]
    schedule_to_close_ms: Option<u64>,
    /// Heartbeat freshness requirement (runner writes heartbeat file periodically)
    #[serde(default)]
    heartbeat_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
struct TimerSpec {
    /// Duration to wait before the step is considered successful (ms)
    duration_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct TimerState {
    due_epoch_ms: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
struct ChildWorkflowSpec {
    spec: String,
    #[serde(default)]
    wait: bool,
    #[serde(default)]
    inherit_search_attributes: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
struct RunConfig {
    #[serde(default)]
    memo: JsonMap<String, Json>,
    #[serde(default)]
    search_attributes: JsonMap<String, Json>,
    #[serde(default)]
    build_id: Option<String>,
    #[serde(default)]
    change_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
struct Defaults {
    #[serde(default)]
    activity_retry: RetryPolicy,
    #[serde(default)]
    timeouts: Timeouts,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct StepSpec {
    id: String,
    /// "plugin.command" or "dev.plugins.check"
    #[serde(default)]
    activity: Option<String>,
    /// Optional child workflow (mutually exclusive with activity/timer)
    #[serde(default)]
    child_workflow: Option<ChildWorkflowSpec>,
    /// Optional timer step (mutually exclusive with activity/child)
    #[serde(default)]
    timer: Option<TimerSpec>,
    #[serde(default)]
    args: Json,
    #[serde(default)]
    idempotent: bool,
    #[serde(default)]
    retry: RetryUnion,
    #[serde(default)]
    timeouts: Timeouts,
    #[serde(default)]
    resources: Vec<String>,
    /// Per-step lock acquisition timeout (ms)
    #[serde(default)]
    lock_timeout_ms: Option<u64>,
    /// Consider lock stale after this age (ms)
    #[serde(default)]
    lock_stale_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct WorkflowSpec {
    version: String,
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    run: RunConfig,
    steps: Vec<StepSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanRow {
    step_id: String,
    activity: String,
    would_run: bool,
    reason: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanSummary {
    spec_id: String,
    steps: Vec<PlanRow>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct RunSummary {
    spec_id: String,
    ran: usize,
    skipped: usize,
    failed: usize,
    started_at: String,
    finished_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ValidateReport {
    spec_id: String,
    errors: Vec<String>,
    warnings: Vec<String>,
    step_count: usize,
    change_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct EffectRecord {
    spec_id: String,
    step_id: String,
    input_sha: String,
    status: String, // "ok" | "failed" | "timeout" | "canceled"
    started_at: String,
    finished_at: String,
    exit_code: i32,
    result_digest: Option<String>,
    last_envelope: Json,
}

/* ---------------- Visibility (T-3) ---------------- */

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
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
    fn describe(&self) -> &'static str {
        "Local workflow runner with idempotent activities, signals, timeouts, and visibility"
    }

    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("workflow")
                .about(self.describe())
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
    ClapCommand::new("validate")
        .about("Validate a workflow spec (schema + invariants)")
        .arg(Arg::new("spec").long("spec").required(true).value_name("YAML")),
)
                .subcommand(
ClapCommand::new("run")
    .about("Run a workflow spec sequentially")
    .arg(Arg::new("spec").long("spec").required(true).value_name("YAML"))
    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
    .arg(Arg::new("allow-change-ids-drift").long("allow-change-ids-drift").action(ArgAction::SetTrue))
    .arg(Arg::new("build-id").long("build-id").value_name("STRING")),
                )
                .subcommand(
                    ClapCommand::new("status")
                        .about("Show effects/history for a spec id (plus visibility if present)")
                        .arg(Arg::new("id").long("id").required(true)),
                )
                // T-1: signals & query
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
                // T-3: visibility list/upsert
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
                                .action(ArgAction::Append)
                                .num_args(1)
                                .value_name("k=v")
                                .required(true),
                        ),
                ),
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            Some(("init", sm)) => {
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
                            timer: None,
                            args: json!({"title":"hello from workflow"}),
                            idempotent: true,
                            retry: RetryUnion::Simple(SimpleRetry {
                                max_attempts: 2,
                                backoff_ms: 250,
                            }),
                            timeouts: Timeouts::default(),
                            resources: vec!["git".into()],
                            child_workflow: None,
                            lock_timeout_ms: None,
                            lock_stale_ms: None,
                        },
                        StepSpec {
                            id: "s2".into(),
                            activity: Some("dev.plugins.check".into()),
                            timer: None,
                            args: json!({}),
                            idempotent: false,
                            retry: RetryUnion::Simple(SimpleRetry::default()),
                            timeouts: Timeouts::default(),
                            resources: vec![],
                            child_workflow: None,
                            lock_timeout_ms: None,
                            lock_stale_ms: None,
                        },
                    ],
                };

                ensure_parent_dir(&out)?;
                fs::write(&out, serde_yaml::to_string(&spec).map_err(anyhow::Error::from)?)?;
                Ok(ctx.ok(
                    "workflow",
                    "init",
                    &json!({"id": id, "spec": out}),
                ))
            }

            Some(("plan", sm)) => {
                let spec_path = sm.get_one::<String>("spec").unwrap();
                let spec = read_spec(spec_path)?;
                let plan = plan_steps(&spec)?;
                Ok(ctx.ok(
                    "workflow",
                    "plan",
                    &PlanSummary {
                        spec_id: spec.id,
                        steps: plan,
                    },
                ))
            }

            Some(("validate", sm)) => {
                let spec_path = sm.get_one::<String>("spec").unwrap();
                // Build a validation report without failing the command
                let mut errors: Vec<String> = vec![];
                let mut warnings: Vec<String> = vec![];
                let mut step_count: usize = 0;
                let mut change_ids: Vec<String> = vec![];
                let txt = match fs::read_to_string(spec_path) {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(format!("read error: {}", e));
                        let rep = ValidateReport {
                            spec_id: "".into(),
                            errors,
                            warnings,
                            step_count: 0,
                            change_ids,
                        };
                        return Ok(ctx.ok("workflow","validate",&rep));
                    }
                };
                match serde_yaml::from_str::<WorkflowSpec>(&txt) {
                    Ok(spec) => {
                        // version
                        if spec.version != "0.1" {
                            errors.push(format!("unsupported version {}", spec.version));
                        }
                        step_count = spec.steps.len();
                        change_ids = spec.run.change_ids.clone();

                        // unique ids + mutually exclusive kinds
                        use std::collections::HashSet;
                        let mut ids = HashSet::new();
                        for s in &spec.steps {
                            if !ids.insert(&s.id) {
                                errors.push(format!("duplicate step id '{}'", s.id));
                            }
                            let set = (s.activity.is_some() as u8)
                                + (s.timer.is_some() as u8)
                                + (s.child_workflow.is_some() as u8);
                            if set == 0 {
                                errors.push(format!("step '{}' must set exactly one of activity|timer|child_workflow", s.id));
                            }
                            if set > 1 {
                                errors.push(format!("step '{}' has multiple kinds; choose only one", s.id));
                            }
                            for r in &s.resources {
                                if !r.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == ':' || c == '-') {
                                    errors.push(format!("step '{}' has invalid resource name '{}'", s.id, r));
                                }
                            }
                        }
                        // change_ids format
                        let re = Regex::new(r"^[A-Z][A-Z0-9_\-]{1,63}$").unwrap();
                        for cid in &spec.run.change_ids {
                            if !re.is_match(cid) {
                                errors.push(format!("invalid change_id '{}': must match {}", cid, re.as_str()));
                            }
                        }
                        if errors.is_empty() && step_count == 0 {
                            warnings.push("no steps defined".into());
                        }
                        let rep = ValidateReport {
                            spec_id: spec.id.clone(),
                            errors, warnings,
                            step_count,
                            change_ids,
                        };
                        Ok(ctx.ok("workflow","validate",&rep))
                    }
                    Err(e) => {
                        errors.push(format!("parse error: {}", e));
                        let rep = ValidateReport {
                            spec_id: "".into(),
                            errors,
                            warnings,
                            step_count: 0,
                            change_ids,
                        };
                        Ok(ctx.ok("workflow","validate",&rep))
                    }
                }
            }

            Some(("run", sm)) => {
                let spec_path = sm.get_one::<String>("spec").unwrap();
                let dry = sm.get_flag("dry-run");
                let allow_drift = sm.get_flag("allow-change-ids-drift");
                let build_id_cli = sm.get_one::<String>("build-id").cloned();
                let mut spec = read_spec(spec_path)?;
                if let Some(bid) = build_id_cli {
                    spec.run.build_id = Some(bid);
                }
                let started_at = now();

                // initialize visibility (T-3)
                let mut vis = Visibility {
                    id: spec.id.clone(),
                    state: "Running".into(),
                    memo: spec.run.memo.clone(),
                    search_attributes: spec.run.search_attributes.clone(),
                    build_id: spec.run.build_id.clone(),
                    used_change_ids: spec.run.change_ids.clone(),
                    started_at: started_at.clone(),
                    updated_at: started_at.clone(),
                    last_step: None,
                    attempts: 0,
                    ran: 0,
                    skipped: 0,
                    failed: 0,
                };
                write_visibility(&spec.id, &vis).ok(); // best-effort

                let plan = plan_steps(&spec)?;
                if dry {
                    return Ok(ctx.ok(
                        "workflow",
                        "run.dryrun",
                        &PlanSummary {
                            spec_id: spec.id,
                            steps: plan,
                        },
                    ));
                }

                let hist = history_path(&spec.id)?;
                ensure_parent_dir(hist.to_str().unwrap())?;

                // Determinism gating (T-2)
                let spec_sha = sha256_file(spec_path)?;
                let prev = read_determinism(&spec.id);
                let mut prev_cids = prev.as_ref().map(|d| d.change_ids.clone()).unwrap_or_default();
                let mut now_cids = spec.run.change_ids.clone();
                prev_cids.sort();
                now_cids.sort();
                if prev.is_some() && prev_cids != now_cids && !allow_drift {
                    append_history(
                        &hist,
                        &json!({"ts": now(), "event":"change_ids_drift", "prev": prev_cids, "now": now_cids}),
                    )?;
                    return Ok(ctx.err(
                        "workflow",
                        "run.change_ids_drift",
                        "workflow.change_ids_drift",
                        &format!("run.change_ids changed; prev={:?} now={:?}", prev_cids, now_cids),
                        Some("--allow-change-ids-drift"),
                        66,
                    ));
                }
                // Persist/update determinism.json
                let now_ts = now();
                let det = Determinism {
                    build_id: spec.run.build_id.clone(),
                    change_ids: spec.run.change_ids.clone(),
                    spec_sha256: spec_sha,
                    first_run_at: prev.as_ref().map(|d| d.first_run_at.clone()).unwrap_or_else(|| now_ts.clone()),
                    last_run_at: now_ts,
                    engine: "local".into(),
                };
                write_determinism(&spec.id, &det).ok();

                let first_scheduled = Instant::now();

                for row in plan {
                    // Handle signals before each step
                    loop {
                        match next_signal_action(&spec.id)? {
                            SignalAction::Stop => {
                                vis.state = "Canceled".into();
                                vis.updated_at = now();
                                write_visibility(&spec.id, &vis).ok();
                                append_history(
                                    &hist,
                                    &json!({"ts": now(), "event":"stop", "at_step": row.step_id}),
                                )?;
                                return Ok(ctx.ok(
                                    "workflow",
                                    "run",
                                    &json!({"status":"canceled","spec_id": spec.id}),
                                ));
                            }
                            SignalAction::Pause => {
                                vis.state = "Paused".into();
                                vis.updated_at = now();
                                write_visibility(&spec.id, &vis).ok();
                                append_history(
                                    &hist,
                                    &json!({"ts": now(), "event":"pause", "at_step": row.step_id}),
                                )?;
                                wait_for_resume(&spec.id)?;
                                vis.state = "Running".into();
                                vis.updated_at = now();
                                write_visibility(&spec.id, &vis).ok();
                                append_history(
                                    &hist,
                                    &json!({"ts": now(), "event":"resume", "at_step": row.step_id}),
                                )?;
                                continue; // re-check signals
                            }
                            SignalAction::Resume | SignalAction::None => break,
                        }
                    }

                    vis.last_step = Some(row.step_id.clone());
                    write_visibility(&spec.id, &vis).ok();

                    if !row.would_run {
                        vis.skipped += 1;
                        vis.updated_at = now();
                        write_visibility(&spec.id, &vis).ok();
                        append_history(
                            &hist,
                            &json!({"ts": now(), "step": row.step_id, "activity": row.activity, "action":"skip", "reason": row.reason }),
                        )?;
                        continue;
                    }

                    // Locate step spec
                    let step = spec
                        .steps
                        .iter()
                        .find(|s| s.id == row.step_id)
                        .unwrap()
                        .clone();

                    // Resolve retry policy and timeouts (step over defaults)
                    let rp = normalize_retry(&step.retry, &spec.defaults.activity_retry);
                    let to = resolve_timeouts(&step.timeouts, &spec.defaults.timeouts);

                    // schedule_to_close window for this step
                    let schedule_deadline = to
                        .schedule_to_close_ms
                        .map(|ms| first_scheduled + Duration::from_millis(ms));

                    let mut input_sha = input_sha(&spec.id, &step);
                    input_sha = salt_input_sha(&input_sha, &spec.run.build_id, &spec.run.change_ids);
                    let mut attempt: u32 = 0;
                    let mut ok = false;

                    // acquire resource locks (if any)
                    let _locks = match acquire_locks_for_step(&spec.id, &step) {
                        Ok(g) => g,
                        Err(e) => {
                            let finished_at = now();
                            let rec = EffectRecord {
                                spec_id: spec.id.clone(),
                                step_id: step.id.clone(),
                                input_sha: input_sha.clone(),
                                status: if format!("{e:?}").contains("lock_timeout") { "timeout".into() } else { "failed".into() },
                                started_at: finished_at.clone(),
                                finished_at: finished_at.clone(),
                                exit_code: 128,
                                result_digest: None,
                                last_envelope: json!({"error":"lock_acquire_failed","detail": format!("{e}")}),
                            };
                            write_effect(&spec.id, &step.id, &rec)?;
                            append_history(
                                &hist,
                                &json!({"ts": finished_at, "step": step.id, "activity": step.activity, "status": rec.status}),
                            )?;
                            vis.failed += 1;
                            vis.updated_at = now();
                            write_visibility(&spec.id, &vis).ok();
                            continue;
                        }
                    };

                    while within_schedule_window(schedule_deadline) {
                        attempt += 1;
                        vis.attempts += 1;
                        write_visibility(&spec.id, &vis).ok();

                        let started = now();
                        let (exit, env, status) = if step.timer.is_some() {
                            run_timer_step(&step, &to, &spec.id, &hist)?
                        } else if step.child_workflow.is_some() {
                            run_child_workflow(&step, &spec, &hist)?
                        } else {
                            exec_activity_with_timeouts(step.activity.as_ref().unwrap(), &step.args, &to, &spec.id, &hist)?
                        };

                        let finished = now();
                        let activity_label = if step.timer.is_some() {
                            "timer".to_string()
                        } else if let Some(c) = &step.child_workflow {
                            match fs::read_to_string(&c.spec) {
                                Ok(txt) => match serde_yaml::from_str::<WorkflowSpec>(&txt) {
                                    Ok(cs) => format!("child:{}", cs.id),
                                    Err(_) => "child_workflow".into(),
                                },
                                Err(_) => "child_workflow".into(),
                            }
                        } else {
                            step.activity.clone().unwrap()
                        };
                        let rec = EffectRecord {
                            spec_id: spec.id.clone(),
                            step_id: step.id.clone(),
                            input_sha: input_sha.clone(),
                            status: status.clone(),
                            started_at: started,
                            finished_at: finished.clone(),
                            exit_code: exit,
                            result_digest: Some(sha256(
                                &serde_json::to_vec(&env).unwrap_or_default(),
                            )),
                            last_envelope: env.clone(),
                        };
                        write_effect(&spec.id, &step.id, &rec)?;
                        append_history(
                            &hist,
                            &json!({"ts": finished, "step": step.id, "activity": activity_label, "attempt": attempt, "exit_code": exit, "status": status}),
                        )?;

                        if status == "ok" {
                            ok = true;
                            break;
                        }

                        if !should_retry(&rp, attempt) {
                            break;
                        }
                        if let Some(slp) = next_backoff_sleep(&rp, attempt) {
                            thread::sleep(slp);
                        }
                    }

                    if ok {
                        vis.ran += 1;
                    } else {
                        vis.failed += 1;
                    }
                    vis.updated_at = now();
                    write_visibility(&spec.id, &vis).ok();
                }

                let finished_at = now();
                let final_state = if vis.failed == 0 {
                    "Completed"
                } else {
                    "Failed"
                };
                vis.state = final_state.into();
                vis.updated_at = finished_at.clone();
                write_visibility(&spec.id, &vis).ok();

                let out = RunSummary {
                    spec_id: spec.id,
                    ran: vis.ran,
                    skipped: vis.skipped,
                    failed: vis.failed,
                    started_at: started_at,
                    finished_at,
                };
                Ok(ctx.ok("workflow", "run", &out))
            }

            Some(("status", sm)) => {
                let id = sm.get_one::<String>("id").unwrap();
                let eff_dir = effects_dir(id);
                let mut effects = vec![];
                if eff_dir.exists() {
                    for ent in walkdir::WalkDir::new(&eff_dir)
                        .min_depth(1)
                        .max_depth(1)
                        .into_iter()
                        .flatten()
                    {
                        let p = ent.path();
                        if p.extension().and_then(|x| x.to_str()) == Some("json") {
                            if let Ok(txt) = fs::read_to_string(p) {
                                if let Ok(v) = serde_json::from_str::<Json>(&txt) {
                                    effects.push(v);
                                }
                            }
                        }
                    }
                }
                let hist = history_path(id)?;
                let history = if hist.exists() {
                    fs::read_to_string(hist).unwrap_or_default()
                } else {
                    String::new()
                };
                let visibility = read_visibility(id);
                Ok(ctx.ok(
                    "workflow",
                    "status",
                    &json!({"id": id, "effects": effects, "history": history, "visibility": visibility}),
                ))
            }

            Some(("signal", sm)) => {
                let id = sm.get_one::<String>("id").unwrap();
                let name = sm.get_one::<String>("name").unwrap();
                let payload_str = sm.get_one::<String>("payload").unwrap();
                let payload: Json = serde_json::from_str(payload_str).unwrap_or(json!({}));

                let dir = signals_dir(id);
                fs::create_dir_all(&dir)?;
                let ts = now().replace(':', "_");
                let path = dir.join(format!("{ts}_{name}.json"));
                fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
                Ok(ctx.ok(
                    "workflow",
                    "signal",
                    &json!({"id": id, "name": name, "file": path}),
                ))
            }

            Some(("query", sm)) => {
                let id = sm.get_one::<String>("id").unwrap();
                let name = sm.get_one::<String>("name").unwrap();
                match name.as_str() {
                    "status" => {
                        let vis = read_visibility(id);
                        Ok(ctx.ok("workflow", "query.status", &vis))
                    }
                    other => Ok(ctx.err(
                        "workflow",
                        "query.unknown",
                        "workflow.query.unknown",
                        &format!("unknown query '{}'", other),
                        None,
                        2,
                    )),
                }
            }

            Some(("list", sm)) => {
                let filt = sm
                    .get_one::<String>("filter")
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let all = read_all_visibility();
                let filtered = if filt.is_empty() {
                    all
                } else {
                    let parsed = parse_filter(filt)?;
                    all.into_iter()
                        .filter(|v| filter_match(v, &parsed))
                        .collect::<Vec<_>>()
                };
                Ok(ctx.ok("workflow", "list", &filtered))
            }

            Some(("upsert-sa", sm)) => {
                let id = sm.get_one::<String>("id").unwrap();
                let kvs = sm
                    .get_many::<String>("attrs")
                    .unwrap()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();

                let mut vis = read_visibility(id).unwrap_or_default();
                vis.id = if vis.id.is_empty() { id.clone() } else { vis.id };
                for kv in kvs {
                    if let Some((k, v)) = kv.split_once('=') {
                        vis.search_attributes
                            .insert(k.to_string(), Json::String(v.to_string()));
                    }
                }
                vis.updated_at = now();
                write_visibility(id, &vis)?;
                Ok(ctx.ok("workflow", "upsert_sa", &vis))
            }

            _ => Ok(ctx.err(
                "workflow",
                "unknown",
                "workflow.unknown",
                "unknown workflow command",
                None,
                2,
            )),
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "workflow.init".into(),
                about: "Create a workflow spec template".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
            },
            ToolSpec {
                name: "workflow.plan".into(),
                about: "Plan (idempotency-aware)".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(PlanSummary),
            },
            ToolSpec {
                name: "workflow.validate".into(),
                about: "Validate a workflow spec (schema + invariants)".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(ValidateReport),
            },
            ToolSpec {
                name: "workflow.run".into(),
                about: "Run a workflow spec sequentially".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(RunSummary),
            },
            ToolSpec {
                name: "workflow.status".into(),
                about: "Show effects, history, visibility".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
            },
            ToolSpec {
                name: "workflow.signal".into(),
                about: "Send a signal to a running workflow".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(serde_json::Value),
            },
            ToolSpec {
                name: "workflow.query".into(),
                about: "Query workflow runtime state".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(Visibility),
            },
            ToolSpec {
                name: "workflow.list".into(),
                about: "List workflows (visibility index)".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(Vec<Visibility>),
            },
            ToolSpec {
                name: "workflow.upsert-sa".into(),
                about: "Upsert search attributes for a workflow".into(),
                args_schema: schema_for!(serde_json::Value),
                result_schema: schema_for!(Visibility),
            },
        ]
    }
}

/* ---------------- Helpers ---------------- */

fn read_spec(path: &str) -> anyhow::Result<WorkflowSpec> {
    let txt = fs::read_to_string(path)?;
    let spec: WorkflowSpec = serde_yaml::from_str(&txt)?;
    if spec.version != "0.1" {
        anyhow::bail!("unsupported spec version: {}", spec.version);
    }
    validate_spec(&spec)?;
    Ok(spec)
}

fn validate_spec(spec: &WorkflowSpec) -> anyhow::Result<()> {
    for s in &spec.steps {
        let has_act = s.activity.as_ref().map(|a| !a.is_empty()).unwrap_or(false);
        let has_timer = s.timer.is_some();
        let has_child = s.child_workflow.is_some();
        let count = (has_act as u8) + (has_timer as u8) + (has_child as u8);
        if count == 0 {
            anyhow::bail!("step '{}' invalid: must set exactly one of 'activity', 'timer', or 'child_workflow'", s.id);
        }
        if count > 1 {
            anyhow::bail!("step '{}' invalid: cannot set more than one of 'activity', 'timer', or 'child_workflow'", s.id);
        }
    }
    Ok(())
}

fn plan_steps(spec: &WorkflowSpec) -> anyhow::Result<Vec<PlanRow>> {
    let mut out = vec![];
    for s in &spec.steps {
        let activity = if s.timer.is_some() {
            "timer".to_string()
        } else if let Some(c) = &s.child_workflow {
            match fs::read_to_string(&c.spec) {
                Ok(txt) => match serde_yaml::from_str::<WorkflowSpec>(&txt) {
                    Ok(cs) => format!("child:{}", cs.id),
                    Err(_) => "child_workflow".into(),
                },
                Err(_) => "child_workflow".into(),
            }
        } else {
            s.activity.clone().unwrap_or_else(|| "activity".to_string())
        };

        let mut input = input_sha(&spec.id, s);
        input = salt_input_sha(&input, &spec.run.build_id, &spec.run.change_ids);
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

fn parse_activity(activity: &str) -> anyhow::Result<(String, Vec<String>)> {
    // Support multi-segment after the first: "dev.plugins.check" -> ("dev", ["plugins","check"])
    let parts = activity.split('.').collect::<Vec<_>>();
    anyhow::ensure!(
        parts.len() >= 2,
        "invalid activity '{}', want plugin.command",
        activity
    );
    let plugin = parts[0].to_string();
    let command_segs = parts[1..].iter().map(|s| s.to_string()).collect::<Vec<_>>();
    Ok((plugin, command_segs))
}

fn exec_activity_with_timeouts(
    activity: &str,
    args: &Json,
    to: &Timeouts,
    spec_id: &str,
    hist: &Path,
) -> anyhow::Result<(i32, Json, String)> {
    let (plugin, command_segs) = parse_activity(activity)?;
    let exe = std::env::current_exe()?;
    let mut argv: Vec<String> = vec![plugin];
    argv.extend(command_segs);
    argv.push("--output".into());
    argv.push("json".into());

    if let Some(obj) = args.as_object() {
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
                        argv.push(
                            item.as_str()
                                .map(|s| s.to_string())
                                .unwrap_or(item.to_string()),
                        );
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

    // Heartbeat path
    let hb_ms = to.heartbeat_ms.unwrap_or(0);
    let hb_path = heartbeat_path(spec_id)?;
    ensure_parent_dir(hb_path.to_str().unwrap())?;

    let mut stdout_buf = Vec::new();
    loop {
        // start_to_close
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

        // Heartbeat
        if hb_ms > 0 {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            if elapsed_ms % hb_ms <= 150 {
                fs::write(&hb_path, now().as_bytes()).ok();
            }
        }

        if let Some(status) = child.try_wait()? {
            if let Some(mut out) = child.stdout.take() {
                out.read_to_end(&mut stdout_buf)?;
            }
            let code = status.code().unwrap_or(-1);
            let val: Json = serde_json::from_slice(&stdout_buf).unwrap_or_else(|_| {
                json!({"stdout": String::from_utf8_lossy(&stdout_buf)})
            });
            let st = if code == 0 { "ok" } else { "failed" }.to_string();
            return Ok((code, val, st));
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn run_timer_step(
    step: &StepSpec,
    to: &Timeouts,
    spec_id: &str,
    hist: &Path,
) -> anyhow::Result<(i32, Json, String)> {
    let t = step
        .timer
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("timer step missing 'timer' field"))?;

    // Determine due time (persisted)
    let st = read_timer_state(spec_id, &step.id);
    let state = if let Some(old) = st {
        old
    } else {
        let due = epoch_ms_now() + (t.duration_ms as i64);
        let ns = TimerState { due_epoch_ms: due };
        write_timer_state(spec_id, &step.id, &ns)?;
        ns
    };

    // Preflight guard for schedule_to_close
    if let Some(s2c) = to.schedule_to_close_ms {
        let remaining_ms = (state.due_epoch_ms - epoch_ms_now()).max(0) as u64;
        if remaining_ms > s2c {
            append_history(
                hist,
                &json!({"ts": now(), "event":"timeout", "kind":"schedule_to_close_preflight"}),
            )?;
            return Ok((124, json!({"error":"schedule_to_close_timeout"}), "timeout".into()));
        }
    }

    let start = Instant::now();
    loop {
        // Signals
        match next_signal_action(spec_id)? {
            SignalAction::Stop => {
                append_history(
                    hist,
                    &json!({"ts": now(), "event":"canceled", "step": step.id}),
                )?;
                return Ok((130, json!({"error":"canceled"}), "canceled".into()));
            }
            SignalAction::Pause => {
                append_history(hist, &json!({"ts": now(), "event":"pause", "step": step.id}))?;
                wait_for_resume(spec_id)?;
                append_history(hist, &json!({"ts": now(), "event":"resume", "step": step.id}))?;
            }
            _ => {}
        }

        // Heartbeat
        if let Some(hb) = to.heartbeat_ms {
            if hb > 0 && start.elapsed().as_millis() as u64 % hb < 200 {
                if let Ok(p) = heartbeat_path(spec_id) {
                    let _ = fs::write(p, now().as_bytes());
                }
            }
        }

        // Start-to-close guard
        if let Some(stc) = to.start_to_close_ms {
            if start.elapsed() >= Duration::from_millis(stc) {
                append_history(
                    hist,
                    &json!({"ts": now(), "event":"timeout", "kind":"start_to_close"}),
                )?;
                return Ok((124, json!({"error":"start_to_close_timeout"}), "timeout".into()));
            }
        }

        // Due time
        let now_ms = epoch_ms_now();
        if now_ms >= state.due_epoch_ms {
            break;
        }
        let remaining = (state.due_epoch_ms - now_ms) as u64;
        let slice = remaining.min(200);
        thread::sleep(Duration::from_millis(slice));
    }

    Ok((0, json!({"timer":"done","at": now()}), "ok".into()))
}

fn run_child_workflow(
    step: &StepSpec,
    parent: &WorkflowSpec,
    hist: &Path,
) -> anyhow::Result<(i32, Json, String)> {
    let c = step.child_workflow.as_ref().ok_or_else(|| anyhow::anyhow!("missing child_workflow spec"))?;
    let child_spec_path = &c.spec;

    // discover child id
    let child_id = match fs::read_to_string(child_spec_path)
        .ok()
        .and_then(|txt| serde_yaml::from_str::<WorkflowSpec>(&txt).ok())
        .map(|cs| cs.id)
    {
        Some(id) => id,
        None => "UNKNOWN".into(),
    };

    append_history(hist, &json!({"ts": now(), "event":"child_start", "child_spec": child_spec_path, "child_id": child_id, "wait": c.wait }))?;

    let exe = std::env::current_exe()?;
    if c.wait {
        let out = Proc::new(exe.clone())
            .args(["workflow","run","--spec", child_spec_path, "--output","json"])
            .output()?;
        let code = out.status.code().unwrap_or(-1);
        let env: Json = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| json!({"stdout": String::from_utf8_lossy(&out.stdout)}));

        // upsert parent's SA if requested
        if c.inherit_search_attributes && child_id != "UNKNOWN" {
            let mut cmd = Proc::new(exe.clone());
            cmd.args(["workflow","upsert-sa","--id",&child_id]);
            for (k, v) in &parent.run.search_attributes {
                let val = match v {
                    Json::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let kv = format!("{}={}", k, val);
                cmd.args(["--attrs", &kv]);
            }
            let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();
        }

        append_history(hist, &json!({"ts": now(), "event":"child_done", "child_id": child_id, "exit_code": code }))?;
        Ok((code, json!({"child_id": child_id, "result": env}), if code==0 { "ok".into() } else { "failed".into() }))
    } else {
        // fire-and-forget
        let _child = Proc::new(exe)
            .args(["workflow","run","--spec", child_spec_path, "--output","json"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // SA inheritance after spawn is racy; skipped for fire-and-forget in v0.
        Ok((0, json!({"child_id": child_id, "spawned": true}), "ok".into()))
    }
}

fn input_sha(spec_id: &str, step: &StepSpec) -> String {
    let mut h = Sha256::new();
    h.update(spec_id.as_bytes());
    h.update(step.id.as_bytes());
    if let Some(act) = &step.activity {
        h.update(act.as_bytes());
    }
    if let Some(t) = &step.timer {
        h.update(b"timer");
        h.update(t.duration_ms.to_le_bytes());
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
fn read_effect(spec_id: &str, step_id: &str) -> Option<EffectRecord> {
    let p = effect_path(spec_id, step_id);
    let txt = fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
}
fn write_effect(spec_id: &str, step_id: &str, rec: &EffectRecord) -> anyhow::Result<()> {
    let dir = effects_dir(spec_id);
    fs::create_dir_all(&dir)?;
    fs::write(effect_path(spec_id, step_id), serde_json::to_vec_pretty(rec)?)?;
    Ok(())
}

fn timers_dir(spec_id: &str) -> PathBuf {
    PathBuf::from(".nexoscli").join("workflows").join(spec_id).join("timers")
}
fn timer_state_path(spec_id: &str, step_id: &str) -> PathBuf {
    timers_dir(spec_id).join(format!("{}.json", step_id))
}
fn read_timer_state(spec_id: &str, step_id: &str) -> Option<TimerState> {
    let p = timer_state_path(spec_id, step_id);
    let txt = fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
}
fn write_timer_state(spec_id: &str, step_id: &str, st: &TimerState) -> anyhow::Result<()> {
    fs::create_dir_all(timers_dir(spec_id))?;
    fs::write(timer_state_path(spec_id, step_id), serde_json::to_vec_pretty(st)?)?;
    Ok(())
}
fn epoch_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

// ------------- Resource Locks (advisory, durable) -------------
fn locks_dir() -> PathBuf {
    PathBuf::from(".nexoscli").join("locks")
}
fn lock_path(resource: &str) -> PathBuf {
    locks_dir().join(format!("{}.json", resource))
}

fn try_acquire_lock(resource: &str, timeout_ms: u64, stale_ms: u64, spec_id: &str, step_id: &str) -> anyhow::Result<bool> {
    use std::time::{Duration, Instant};
    let p = lock_path(resource);
    let start = Instant::now();
    loop {
        // try atomic create
        match fs::OpenOptions::new().create_new(true).write(true).open(&p) {
            Ok(mut f) => {
                use std::io::Write as _;
                let body = json!({
                    "resource": resource,
                    "spec_id": spec_id,
                    "step_id": step_id,
                    "pid": std::process::id(),
                    "acquired_at": now(),
                    "updated_at": now()
                });
                f.write_all(serde_json::to_vec_pretty(&body)?.as_slice())?;
                return Ok(true);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // stale check
                if let Ok(txt) = fs::read_to_string(&p) {
                    if let Ok(v) = serde_json::from_str::<Json>(&txt) {
                        let updated = v.get("updated_at").and_then(|x| x.as_str()).unwrap_or("");
                        if let Ok(t) = OffsetDateTime::parse(updated, &Rfc3339) {
                            let age_ms = (OffsetDateTime::now_utc() - t).whole_milliseconds().max(0) as u64;
                            if age_ms > stale_ms {
                                let _ = fs::remove_file(&p); // best effort
                                continue; // try again
                            }
                        }
                    }
                }
                if start.elapsed() >= Duration::from_millis(timeout_ms) { return Ok(false); }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn release_lock(resource: &str, spec_id: &str, step_id: &str) -> anyhow::Result<()> {
    let p = lock_path(resource);
    // best-effort owner check
    if let Ok(txt) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<Json>(&txt) {
            let ok_owner =
                v.get("spec_id").and_then(|x| x.as_str()) == Some(spec_id) &&
                v.get("step_id").and_then(|x| x.as_str()) == Some(step_id);
            if ok_owner {
                let _ = fs::remove_file(&p);
                return Ok(());
            }
        }
    }
    Ok(()) // ignore if not ours
}

struct LockGuard { spec_id: String, step_id: String, resources: Vec<String> }
impl Drop for LockGuard {
    fn drop(&mut self) {
        for r in &self.resources { let _ = release_lock(r, &self.spec_id, &self.step_id); }
    }
}

fn acquire_locks_for_step(spec_id: &str, step: &StepSpec) -> anyhow::Result<Option<LockGuard>> {
    if step.resources.is_empty() { return Ok(None); }
    let timeout = step.lock_timeout_ms.unwrap_or(5_000);
    let stale   = step.lock_stale_ms.unwrap_or(600_000);
    let mut res = step.resources.clone();
    res.sort();
    res.dedup();
    fs::create_dir_all(locks_dir())?;

    let mut acquired = Vec::new();
    for r in res {
        if try_acquire_lock(&r, timeout, stale, spec_id, &step.id)? {
            acquired.push(r);
        } else {
            // release what we got and fail
            for a in &acquired { let _ = release_lock(a, spec_id, &step.id); }
            anyhow::bail!("lock_timeout");
        }
    }
    Ok(Some(LockGuard { spec_id: spec_id.to_string(), step_id: step.id.clone(), resources: acquired }))
}

fn history_path(spec_id: &str) -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(".nexoscli")
        .join("workflows")
        .join(spec_id)
        .join("history.jsonl"))
}
fn append_history(path: &Path, row: &Json) -> anyhow::Result<()> {
    ensure_parent_dir(path.to_str().unwrap())?;
    use std::io::Write as _;
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
fn read_visibility(spec_id: &str) -> Option<Visibility> {
    let p = visibility_path(spec_id).ok()?;
    let txt = fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
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

 // ------------- Determinism (T-2) -------------
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Determinism {
    build_id: Option<String>,
    change_ids: Vec<String>,
    spec_sha256: String,
    first_run_at: String,
    last_run_at: String,
    engine: String, // "local" | "temporal"
}
fn determinism_path(spec_id: &str) -> PathBuf {
    PathBuf::from(".nexoscli").join("workflows").join(spec_id).join("determinism.json")
}
fn read_determinism(spec_id: &str) -> Option<Determinism> {
    let p = determinism_path(spec_id);
    let txt = fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
}
fn write_determinism(spec_id: &str, det: &Determinism) -> anyhow::Result<()> {
    let p = determinism_path(spec_id);
    ensure_parent_dir(p.to_str().unwrap())?;
    fs::write(p, serde_json::to_vec_pretty(det)?)?;
    Ok(())
}
fn sha256_file(path: &str) -> anyhow::Result<String> {
    let b = fs::read(path)?;
    Ok(format!("sha256:{:x}", Sha256::digest(&b)))
}
fn salt_input_sha(base: &str, build_id: &Option<String>, change_ids: &Vec<String>) -> String {
    let mut h = Sha256::new();
    h.update(base.as_bytes());
    if let Some(b) = build_id {
        h.update(b.as_bytes());
    }
    for cid in change_ids {
        h.update(cid.as_bytes());
    }
    format!("sha256:{:x}", h.finalize())
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
    // Coalesce: stop > pause > resume
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
                "pause" => {
                    if action != SignalAction::Stop {
                        SignalAction::Pause
                    } else {
                        action
                    }
                }
                "resume" => {
                    if matches!(action, SignalAction::None | SignalAction::Resume) {
                        SignalAction::Resume
                    } else {
                        action
                    }
                }
                _ => action,
            };
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
    // key=value [AND key=value]*
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

/* ---------------- Policy Helpers ---------------- */

fn normalize_retry(u: &RetryUnion, defaults: &RetryPolicy) -> RetryPolicy {
    match u {
        RetryUnion::Simple(s) => RetryPolicy {
            use_default: false,
            initial_interval_ms: Some(s.backoff_ms),
            backoff_coefficient: Some(1.0),
            maximum_interval_ms: Some(s.backoff_ms),
            maximum_attempts: Some(s.max_attempts),
        },
        RetryUnion::Temporal(rp) => {
            if rp.use_default {
                defaults.clone()
            } else {
                RetryPolicy {
                    use_default: false,
                    initial_interval_ms: rp.initial_interval_ms.or(defaults.initial_interval_ms),
                    backoff_coefficient: rp
                        .backoff_coefficient
                        .or(defaults.backoff_coefficient),
                    maximum_interval_ms: rp.maximum_interval_ms.or(defaults.maximum_interval_ms),
                    maximum_attempts: rp.maximum_attempts.or(defaults.maximum_attempts),
                }
            }
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
        Some(0) | None => true,
        Some(n) => attempt < n,
    }
}
fn next_backoff_sleep(rp: &RetryPolicy, attempt: u32) -> Option<Duration> {
    let base = rp.initial_interval_ms.unwrap_or(500) as f64;
    let coeff = rp.backoff_coefficient.unwrap_or(2.0);
    let max = rp.maximum_interval_ms.unwrap_or(30_000) as f64;
    let exp = (attempt.saturating_sub(1)) as f64;
    let mut ms = (base * coeff.powf(exp)).round();
    if ms > max {
        ms = max;
    }
    Some(Duration::from_millis(ms as u64))
}
fn within_schedule_window(deadline: Option<Instant>) -> bool {
    match deadline {
        None => true,
        Some(d) => Instant::now() < d,
    }
}

/* ---------------- Util ---------------- */

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
fn sha256(b: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(b))
}
fn current_build_id() -> String {
    if let Ok(s) = std::env::var("GIT_COMMIT") {
        return format!("git:{s}");
    }
    format!("pkg:{}", env!("CARGO_PKG_VERSION"))
}
fn ensure_parent_dir(path: &str) -> anyhow::Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}
