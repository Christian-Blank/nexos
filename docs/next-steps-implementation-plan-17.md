Awesome—zooming back out to the critical path toward Temporal, you’ve already knocked out:
	•	stable envelopes + event log + telemetry,
	•	task lifecycle + session correlation,
	•	replay/bundling (TracePacks),
	•	Cline ingest with doctor/CI checks.

Next step (most leverage for Temporal):
Introduce a minimal “Workflow” layer with idempotent activities and a local runner.
This gives us Temporal‑shaped surfaces (workflow spec, activities, retries, history, idempotency) without pulling in the Temporal SDK yet. When we’re ready, we’ll swap the local runner for a Temporal worker with the same activity contracts.

Below I give you an implementable slice:
	•	a new workflow plugin (spec schema, plan/run/status),
	•	an effects ledger for idempotency (so activities are safe to replay),
	•	a local activity runner that executes existing CLI commands (plugin.command) with JSON output,
	•	small patches to wire it in.

⸻

What you’ll get in this slice
	•	Workflow Spec (YAML):

version: 0.1
id: WF-demo
name: Demo workflow
steps:
  - id: s1
    activity: issue.new         # maps to: nexoscli issue new --title ...
    args:
      title: "hello from workflow"
    idempotent: true
    retry: { max_attempts: 2, backoff_ms: 250 }
  - id: s2
    activity: dev.plugins.check # maps to: nexoscli dev plugins check
    args: { }
    idempotent: false

	•	Local runner that:
	•	Plans which steps will run vs be skipped (based on idempotent effects).
	•	Executes steps sequentially with retry policy.
	•	Logs history under .nexoscli/workflows/<wf_id>/history.jsonl.
	•	Maintains an effects ledger in .nexoscli/effects/<wf_id>/<step_id>.json with input hash + result digest to make replays deterministic and safe.
	•	Activity adapter that turns activity: "plugin.command" + args: {...} into a CLI call to your existing commands with --output json, parsing the envelope.

This aligns tightly with Temporal’s model:
	•	Workflow ↔ YAML spec + run/history
	•	Activities ↔ CLI calls with stable JSON contracts
	•	Determinism/Replay ↔ effects ledger + idempotency keys
	•	Retries/Backoff ↔ per‑step retry policy
	•	Signals/heartbeats → trivial to add next (file‑based “pause/stop” + periodic heartbeats)

⸻

1) Add the new plugin

plugins/workflow/Cargo.toml

[package]
name = "nexoscli-plugin-workflow"
version = "0.1.0"
edition = "2021"

[dependencies]
nexoscli-core = { path = "../../nexoscli-core" }
anyhow         = { workspace = true }
serde          = { workspace = true, features = ["derive"] }
serde_json     = { workspace = true }
serde_yaml     = "0.9"
schemars       = { workspace = true }
clap           = { workspace = true }
time           = { workspace = true }
walkdir        = { workspace = true }
sha2           = { workspace = true }
hex            = { workspace = true }

plugins/workflow/src/lib.rs

use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand};
use nexoscli_core::{plugin::{Plugin, CommandContext, ToolSpec}, Result};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::{Path, PathBuf}, process::Command as Proc};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use sha2::{Digest, Sha256};

pub struct WorkflowPlugin;
impl WorkflowPlugin { pub fn new() -> Self { Self } }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct Retry {
    #[serde(default)] max_attempts: u32,
    #[serde(default)] backoff_ms: u64,
}
impl Default for Retry { fn default() -> Self { Self{ max_attempts: 1, backoff_ms: 0 } } }

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct StepSpec {
    id: String,
    activity: String,               // "plugin.command"
    #[serde(default)] args: serde_json::Value,
    #[serde(default)] idempotent: bool,
    #[serde(default)] retry: Retry,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct WorkflowSpec {
    version: String,
    id: String,
    #[serde(default)] name: Option<String>,
    steps: Vec<StepSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct PlanRow {
    step_id: String,
    activity: String,
    would_run: bool,          // false if idempotent + effect match
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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct EffectRecord {
    spec_id: String,
    step_id: String,
    input_sha: String,        // hash(activity + normalized args)
    status: String,           // "ok" | "failed"
    started_at: String,
    finished_at: String,
    exit_code: i32,
    result_digest: Option<String>,
    last_envelope: serde_json::Value, // last JSON envelope (ok or err)
}

/* ---------------- Plugin ---------------- */

impl Plugin for WorkflowPlugin {
    fn name(&self) -> &'static str { "workflow" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn describe(&self) -> &'static str { "Local workflow runner with idempotent activities" }

    fn augment_commands(&self, app: ClapCommand) -> ClapCommand {
        app.subcommand(
            ClapCommand::new("workflow").about(self.describe())
              .subcommand(
                  ClapCommand::new("init")
                    .about("Create a workflow spec template")
                    .arg(Arg::new("id").long("id").required(true))
                    .arg(Arg::new("out").long("out").value_name("PATH")
                         .help("Defaults to .nexoscli/workflows/<id>/spec.yaml"))
              )
              .subcommand(
                  ClapCommand::new("plan")
                    .about("Plan which steps would run (idempotency-aware)")
                    .arg(Arg::new("spec").long("spec").required(true).value_name("YAML"))
              )
              .subcommand(
                  ClapCommand::new("run")
                    .about("Run a workflow spec sequentially")
                    .arg(Arg::new("spec").long("spec").required(true).value_name("YAML"))
                    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
              )
              .subcommand(
                  ClapCommand::new("status")
                    .about("Show effects/history for a spec id")
                    .arg(Arg::new("id").long("id").required(true))
              )
        )
    }

    fn execute(&self, m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
        match m.subcommand() {
            Some(("init", sm)) => {
                let id = sm.get_one::<String>("id").unwrap().to_string();
                let out = sm.get_one::<String>("out").cloned()
                    .unwrap_or_else(|| format!(".nexoscli/workflows/{}/spec.yaml", id));
                let spec = WorkflowSpec {
                    version: "0.1".into(),
                    id: id.clone(),
                    name: Some(format!("{} workflow", id)),
                    steps: vec![
                        StepSpec{
                            id:"s1".into(), activity:"issue.new".into(),
                            args: json!({"title":"hello from workflow"}), idempotent:true, retry: Retry{max_attempts:2, backoff_ms:250}
                        },
                        StepSpec{
                            id:"s2".into(), activity:"dev.plugins.check".into(),
                            args: json!({}), idempotent:false, retry: Retry::default()
                        }
                    ]
                };
                ensure_parent_dir(&out)?;
                fs::write(&out, serde_yaml::to_string(&spec)?)?;
                Ok(ctx.ok("workflow","init",&json!({"id": id, "spec": out})))
            }

            Some(("plan", sm)) => {
                let spec_path = sm.get_one::<String>("spec").unwrap();
                let spec = read_spec(spec_path)?;
                let plan = plan_steps(&spec)?;
                Ok(ctx.ok("workflow","plan",&PlanSummary{ spec_id: spec.id, steps: plan }))
            }

            Some(("run", sm)) => {
                let spec_path = sm.get_one::<String>("spec").unwrap();
                let dry = sm.get_flag("dry-run");
                let spec = read_spec(spec_path)?;
                let start = now();

                let plan = plan_steps(&spec)?;
                if dry {
                    return Ok(ctx.ok("workflow","run.dryrun",&PlanSummary{ spec_id: spec.id, steps: plan }));
                }

                let mut ran = 0usize;
                let mut skipped = 0usize;
                let mut failed = 0usize;

                // history log
                let hist = history_path(&spec.id)?;
                ensure_parent_dir(hist.to_str().unwrap())?;

                for row in plan {
                    if !row.would_run {
                        skipped += 1;
                        append_history(&hist, &json!({"ts": now(), "step": row.step_id, "activity": row.activity, "action":"skip", "reason": row.reason }))?;
                        continue;
                    }

                    // execute with retry
                    let step = spec.steps.iter().find(|s| s.id==row.step_id).unwrap().clone();
                    let input_sha = input_sha(&spec.id, &step);
                    let mut attempt = 0u32;
                    let mut last_env = json!({});
                    let mut last_exit = 0i32;
                    let mut ok = false;

                    while attempt < step.retry.max_attempts.max(1) {
                        attempt += 1;
                        let started = now();
                        let (exit, env) = exec_activity(&step.activity, &step.args)?;
                        last_env = env.clone();
                        last_exit = exit;
                        let finished = now();

                        let status = if exit==0 { "ok" } else { "failed" };
                        let rec = EffectRecord {
                            spec_id: spec.id.clone(),
                            step_id: step.id.clone(),
                            input_sha: input_sha.clone(),
                            status: status.into(),
                            started_at: started,
                            finished_at: finished,
                            exit_code: exit,
                            result_digest: Some(sha256(&serde_json::to_vec(&env).unwrap_or_default())),
                            last_envelope: env,
                        };
                        write_effect(&spec.id, &step.id, &rec)?;

                        append_history(&hist, &json!({
                            "ts": finished, "step": step.id, "activity": step.activity,
                            "attempt": attempt, "exit_code": exit, "status": status
                        }))?;

                        if exit == 0 {
                            ok = true;
                            break;
                        }
                        if step.retry.backoff_ms > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(step.retry.backoff_ms));
                        }
                    }

                    if ok { ran += 1; } else { failed += 1; }
                }

                let out = RunSummary {
                    spec_id: spec.id,
                    ran, skipped, failed,
                    started_at: start,
                    finished_at: now(),
                };
                Ok(ctx.ok("workflow","run",&out))
            }

            Some(("status", sm)) => {
                let id = sm.get_one::<String>("id").unwrap();
                let eff_dir = effects_dir(id);
                let mut effects = vec![];
                if eff_dir.exists() {
                    for ent in walkdir::WalkDir::new(&eff_dir).min_depth(1).max_depth(1).into_iter().flatten() {
                        let p = ent.path();
                        if p.extension().and_then(|x| x.to_str()) == Some("json") {
                            if let Ok(txt) = fs::read_to_string(p) {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                                    effects.push(v);
                                }
                            }
                        }
                    }
                }
                let hist = history_path(id)?;
                let history = if hist.exists() { fs::read_to_string(hist).unwrap_or_default() } else { String::new() };
                Ok(ctx.ok("workflow","status",&json!({"id": id, "effects": effects, "history": history})))
            }

            _ => Ok(ctx.err("workflow","unknown","workflow.unknown","unknown workflow command", None, 2))
        }
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec{ name:"workflow.init".into(), about:"Create a workflow spec template".into(),
                args_schema: schema_for!(serde_json::Value), result_schema: schema_for!(serde_json::Value) },
            ToolSpec{ name:"workflow.plan".into(), about:"Plan (idempotency-aware)".into(),
                args_schema: schema_for!(serde_json::Value), result_schema: schema_for!(PlanSummary) },
            ToolSpec{ name:"workflow.run".into(),  about:"Run a workflow spec sequentially".into(),
                args_schema: schema_for!(serde_json::Value), result_schema: schema_for!(RunSummary) },
            ToolSpec{ name:"workflow.status".into(), about:"Show effects and history for a spec".into(),
                args_schema: schema_for!(serde_json::Value), result_schema: schema_for!(serde_json::Value) },
        ]
    }
}

/* ---------------- helpers ---------------- */

fn read_spec(path: &str) -> anyhow::Result<WorkflowSpec> {
    let txt = fs::read_to_string(path)?;
    let spec: WorkflowSpec = serde_yaml::from_str(&txt)?;
    if spec.version != "0.1" { anyhow::bail!("unsupported spec version: {}", spec.version); }
    Ok(spec)
}

fn plan_steps(spec: &WorkflowSpec) -> anyhow::Result<Vec<PlanRow>> {
    let mut out = vec![];
    for s in &spec.steps {
        let input = input_sha(&spec.id, s);
        let eff = read_effect(&spec.id, &s.id);
        let (would_run, reason) = if s.idempotent {
            match eff {
                None => (true, "no prior effect".into()),
                Some(rec) if rec.input_sha != input => (true, "input changed".into()),
                Some(rec) if rec.status != "ok"     => (true, "prior failed".into()),
                Some(_) => (false, "idempotent match".into()),
            }
        } else {
            (true, "non-idempotent".into())
        };
        out.push(PlanRow{
            step_id: s.id.clone(),
            activity: s.activity.clone(),
            would_run, reason
        });
    }
    Ok(out)
}

fn exec_activity(activity: &str, args: &serde_json::Value) -> anyhow::Result<(i32, serde_json::Value)> {
    // activity "plugin.command" -> ["plugin","command"]
    let mut parts = activity.split('.').collect::<Vec<_>>();
    anyhow::ensure!(parts.len()==2, "invalid activity '{}', want plugin.command", activity);
    let plugin = parts.remove(0);
    let command = parts.remove(0);

    let exe = std::env::current_exe()?;
    let mut argv: Vec<String> = vec![plugin.to_string(), command.to_string(), "--output".into(), "json".into()];
    // naive arg mapping: object fields -> --key value ; true -> --key
    if let Some(obj) = args.as_object() {
        for (k,v) in obj {
            match v {
                serde_json::Value::Bool(true) => { argv.push(format!("--{}", k)); },
                serde_json::Value::Bool(false) => { /* skip */ }
                serde_json::Value::String(s) => { argv.push(format!("--{}", k)); argv.push(s.clone()); }
                serde_json::Value::Number(n) => { argv.push(format!("--{}", k)); argv.push(n.to_string()); }
                serde_json::Value::Array(a) => {
                    for item in a {
                        argv.push(format!("--{}", k));
                        argv.push(item.as_str().map(|s| s.to_string()).unwrap_or(item.to_string()));
                    }
                }
                other => {
                    // fallback: pass as JSON-encoded string
                    argv.push(format!("--{}", k));
                    argv.push(serde_json::to_string(other)?);
                }
            }
        }
    }

    let out = Proc::new(exe).args(&argv).output()?;
    let code = out.status.code().unwrap_or(-1);
    // tolerate non-JSON (but our CLI should be JSON)
    let val: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| json!({"stdout": String::from_utf8_lossy(&out.stdout)}));
    Ok((code, val))
}

fn input_sha(spec_id: &str, step: &StepSpec) -> String {
    let mut h = Sha256::new();
    h.update(spec_id.as_bytes());
    h.update(step.id.as_bytes());
    h.update(step.activity.as_bytes());
    let norm = serde_json::to_vec(&step.args).unwrap_or_default();
    h.update(&norm);
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

fn history_path(spec_id: &str) -> anyhow::Result<PathBuf> {
    let p = PathBuf::from(".nexoscli").join("workflows").join(spec_id).join("history.jsonl");
    Ok(p)
}
fn append_history(path: &Path, row: &serde_json::Value) -> anyhow::Result<()> {
    ensure_parent_dir(path.to_str().unwrap())?;
    use std::io::Write as _;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(row)?)?;
    Ok(())
}

fn now() -> String { OffsetDateTime::now_utc().format(&Rfc3339).unwrap() }
fn sha256(b: &[u8]) -> String { format!("sha256:{:x}", Sha256::digest(b)) }

fn ensure_parent_dir(path: &str) -> anyhow::Result<()> {
    if let Some(dir) = Path::new(path).parent() { fs::create_dir_all(dir)?; }
    Ok(())
}


⸻

2) Wire it up

1) Add the crate to your workspace (top-level Cargo.toml):

[workspace]
members = [
    # ... existing
    "plugins/workflow",
]

2) Add dependency for the bin (nexoscli-bin/Cargo.toml):

[dependencies]
# ... existing
nexoscli-plugin-workflow = { path = "../plugins/workflow" }

3) Register plugin in nexoscli-bin/src/main.rs:

// after other use lines
use nexoscli_plugin_workflow::WorkflowPlugin;

// in main(), after creating registry and other registers:
registry.register(Box::new(WorkflowPlugin::new()));


⸻

3) Quick smoke

# Build
cargo build --workspace

# Init and a small spec
./target/debug/nexoscli dev init --yes
echo 'patterns: []' > .nexoscli/redact.yaml

./target/debug/nexoscli workflow init --id WF-demo
cat .nexoscli/workflows/WF-demo/spec.yaml

# Plan (should show s1=would_run, s2=would_run)
./target/debug/nexoscli workflow plan --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

# Run
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

# Re-run (s1 should be skipped if idempotent and same args)
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

# Inspect effects/history
./target/debug/nexoscli workflow status --id WF-demo --output json


⸻

4) Why this is the right “next”
	•	Idempotent activities → safe deterministic replay / retries (Temporal’s core guarantee).
	•	Effect ledger → gives you exactly-once semantics at the CLI boundary (Temporal activity replays won’t duplicate side-effects).
	•	Spec → plan → run → mirrors Temporal’s Workflow Definition & Execution model.
	•	Separation of concerns: activities are just your existing CLI contracts; the runner orchestrates.

When you later adopt Temporal:
	•	Keep the spec + activity mapping.
	•	Swap exec_activity() to a Temporal activity call (or run a Temporal worker that shells out to the same CLI invocations).
	•	Use heartbeats (Temporal) and our file-based heartbeat as a compatibility layer.
	•	Effects ledger remains your barrier against accidental double‑apply.

⸻

5) Near‑term follow‑ups (small bites) add to ROADMAP_NEXT.md as suggestions
	1.	Heartbeats & control
	•	Write .nexoscli/workflows/<id>/heartbeat every N seconds during step exec.
	•	Obey .nexoscli/workflows/<id>/control = pause|stop (for manual signals).
	2.	Resource locks (proto)
	•	.nexoscli/locks/<resource> advisory lock file with pid+ts. Steps can declare resources: ["git"].
	3.	Spec validation & schema export
	•	Add workflow.validate and a JSON Schema output so agents can author specs confidently.
	4.	Status & UI
	•	Enrich status with per‑step last result + attempt count + last error.
	5.	First DRI workflows
	•	Encode a “Rules DRI” workflow (lint, export, doctor ci) as a YAML under docs/workflows/.
