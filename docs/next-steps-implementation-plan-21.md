Absolutely—staying laser‑focused on the Temporal‑ready foundation first is the right call. Here’s the rest of the critical‑path roadmap and then a drop‑in implementation for the logical next step (Change‑IDs + Build‑ID + Validate/Gate). This keeps replay deterministic as your code/specs evolve and is the last big guardrail before flipping the Temporal switch.

⸻

Critical path to “flip the Temporal switch”

You’ve already landed: stable envelopes, events/telemetry, task lifecycle + session correlation, TracePacks, Cline ingest, local workflow runner with idempotent activities, timers, signals/queries, visibility, resource locks, child workflows.

T‑2 — Determinism under evolution (this step)
	•	Change‑IDs (workflow versioning fence) recorded & enforced
	•	Build‑ID pinned into visibility for routing/compatibility
	•	Validate: schema + invariants command (workflow validate)
	•	Gate: run fails on change_ids drift unless explicitly allowed

T‑1 — Temporal adapter (thin)
	•	Add --engine temporal to workflow run
	•	Emit outbox envelopes to .nexoscli/engine/outbox/:
	•	workflow.start, workflow.signal, activity.schedule, child.start, timer.schedule
	•	Include memo, search attributes, build_id, change_ids, spec_sha
	•	Do not run steps locally in temporal mode

T‑0 — Cutover smoke
	•	Minimal worker (one activity + one timer + child) against a dev namespace
	•	Prove: idempotency via effects ledger (no duplicate side‑effects on replay)
	•	Prove: search attributes & memo visible, listable, queryable

After T‑0, we pick up the Cline/human/AI workflows and reuse the same spec surfaces—just different engines.

⸻

T‑2 Implementation: Change‑IDs + Build‑ID + Validate/Gate

What you get in this drop
	•	run block on WorkflowSpec (memo, search_attributes, build_id, change_ids)
	•	Determinism file: .nexoscli/workflows/<wf_id>/determinism.json
	•	Gating: workflow run fails fast on change_ids drift
	•	workflow validate to preflight specs & invariants
	•	Visibility enriched with build_id + used_change_ids (already referenced in your docs)

Below are copy‑paste patches for plugins/workflow/src/lib.rs and a couple of tiny doc/tests adds. They compose with your current file (timers/locks/child workflows already in).

⸻

1) Extend the spec schema

Add these types near your other spec structs:

use std::collections::BTreeMap;
use serde_json::Value as Json;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct RunConfig {
    #[serde(default)] memo: BTreeMap<String, Json>,
    #[serde(default)] search_attributes: BTreeMap<String, String>,
    #[serde(default)] build_id: Option<String>,
    #[serde(default)] change_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Timeouts {
    #[serde(default)] start_to_close_ms: Option<u64>,
    #[serde(default)] schedule_to_close_ms: Option<u64>,
    #[serde(default)] heartbeat_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ChildWorkflowSpec {
    spec: String,
    #[serde(default)] wait: bool,
    #[serde(default)] inherit_search_attributes: bool,
}

Update WorkflowSpec and StepSpec:

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct WorkflowSpec {
    version: String,
    id: String,
    #[serde(default)] name: Option<String>,
    #[serde(default)] run: RunConfig,          // ← NEW
    #[serde(default)] steps: Vec<StepSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct StepSpec {
    id: String,
    #[serde(default)] activity: Option<String>,
    #[serde(default)] child_workflow: Option<ChildWorkflowSpec>,
    #[serde(default)] timer: Option<TimerSpec>,
    #[serde(default)] args: Json,
    #[serde(default)] idempotent: bool,
    #[serde(default)] retry: RetryPolicy,      // if you still use Retry, keep an alias
    #[serde(default)] timeouts: Timeouts,
    #[serde(default)] resources: Vec<String>,
    #[serde(default)] lock_timeout_ms: Option<u64>,
    #[serde(default)] lock_stale_ms: Option<u64>,
}

(If your project still uses the simpler Retry struct, keep it; the names here match your prior patches—use your actual retry type.)

⸻

2) Determinism file helpers

Add these helpers near your other FS helpers:

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct Determinism {
    build_id: Option<String>,
    change_ids: Vec<String>,
    spec_sha256: String,
    first_run_at: String,
    last_run_at: String,
    engine: String, // "local" or later "temporal"
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


⸻

3) workflow validate command

Add a new subcommand in augment_commands:

.subcommand(
  ClapCommand::new("validate")
    .about("Validate a workflow spec (schema + invariants)")
    .arg(Arg::new("spec").long("spec").required(true).value_name("YAML"))
)

Handle it in execute:

Some(("validate", sm)) => {
    let spec_path = sm.get_one::<String>("spec").unwrap();
    let spec = read_spec(spec_path)?;
    let report = validate_spec(&spec)?;
    Ok(ctx.ok("workflow","validate",&report))
}

Validator (add function):

#[derive(Debug, Serialize)]
struct ValidateReport {
    spec_id: String,
    errors: Vec<String>,
    warnings: Vec<String>,
    step_count: usize,
    change_ids: Vec<String>,
}

fn validate_spec(spec: &WorkflowSpec) -> anyhow::Result<ValidateReport> {
    let mut errors = vec![];
    let mut warnings = vec![];

    if spec.version != "0.1" {
        errors.push(format!("unsupported version {}", spec.version));
    }

    // unique step ids & mutual exclusivity
    use std::collections::HashSet;
    let mut ids = HashSet::new();
    for s in &spec.steps {
        if !ids.insert(&s.id) { errors.push(format!("duplicate step id '{}'", s.id)); }
        let set = (s.activity.is_some() as u8) + (s.timer.is_some() as u8) + (s.child_workflow.is_some() as u8);
        if set == 0 { errors.push(format!("step '{}' must have one of: activity|timer|child_workflow", s.id)); }
        if set > 1 { errors.push(format!("step '{}' has multiple kinds; choose only one", s.id)); }
        // resource names
        for r in &s.resources {
            if !r.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == ':' || c == '-' ) {
                errors.push(format!("step '{}' has invalid resource name '{}'", s.id, r));
            }
        }
    }

    // change_ids format
    let re = regex::Regex::new(r"^[A-Z][A-Z0-9_\-]{1,63}$").unwrap();
    for cid in &spec.run.change_ids {
        if !re.is_match(cid) {
            errors.push(format!("invalid change_id '{}': must match {}", cid, re.as_str()));
        }
    }

    if errors.is_empty() && spec.steps.is_empty() {
        warnings.push("no steps defined".into());
    }

    Ok(ValidateReport {
        spec_id: spec.id.clone(),
        errors, warnings,
        step_count: spec.steps.len(),
        change_ids: spec.run.change_ids.clone(),
    })
}

(Add regex = { workspace = true } to the plugin’s Cargo.toml if not already.)

⸻

4) Gating inside workflow run
	•	Add a flag to allow override when you really intend to change the set:

In augment_commands for run:

.arg(Arg::new("allow-change-ids-drift").long("allow-change-ids-drift").action(ArgAction::SetTrue))
.arg(Arg::new("build-id").long("build-id").value_name("STRING"))

	•	In the run handler, before planning steps:

let allow_drift = sm.get_flag("allow-change-ids-drift");
let build_id_cli = sm.get_one::<String>("build-id").cloned();

let spec_sha = sha256_file(spec_path)?;
let mut spec = read_spec(spec_path)?;
if build_id_cli.is_some() {
    spec.run.build_id = build_id_cli;
}

// determinism gate
let det_prev = read_determinism(&spec.id);
let now_ts = now();
let change_ids = spec.run.change_ids.clone();
let det_new = Determinism {
    build_id: spec.run.build_id.clone(),
    change_ids: change_ids.clone(),
    spec_sha256: spec_sha.clone(),
    first_run_at: det_prev.as_ref().map(|d| d.first_run_at.clone()).unwrap_or_else(|| now_ts.clone()),
    last_run_at: now_ts.clone(),
    engine: "local".into(),
};

// drift detection
if let Some(prev) = &det_prev {
    // compare sets (order-insensitive)
    let mut a = prev.change_ids.clone(); a.sort();
    let mut b = change_ids.clone(); b.sort();
    if a != b && !allow_drift {
        append_history(&hist, &json!({
            "ts": now(), "event":"change_ids_drift",
            "prev": a, "now": b
        }))?;
        return Ok(ctx.err("workflow","run.change_ids_drift",
            "workflow.change_ids_drift",
            "run.change_ids changed; pass --allow-change-ids-drift to proceed",
            Some(json!({"prev": a, "now": b})),
            66));
    }
}

// record determinism (create or update)
write_determinism(&spec.id, &det_new)?;

	•	Visibility: when you write/update visibility.json, include:

vis.build_id = spec.run.build_id.clone();
vis.used_change_ids = change_ids.clone();

(Add fields to your visibility struct as needed; your docs already refer to them.)

⸻

5) Input hash includes the run.change_ids & build_id

Update input_sha(...) so determinism is baked into idempotency:

fn input_sha(spec_id: &str, step: &StepSpec) -> String {
    let mut h = Sha256::new();
    h.update(spec_id.as_bytes());
    h.update(step.id.as_bytes());
    if let Some(act) = &step.activity { h.update(act.as_bytes()); }
    if let Some(t) = &step.timer { h.update(b"timer"); h.update(t.duration_ms.to_le_bytes()); }
    // include args
    h.update(&serde_json::to_vec(&step.args).unwrap_or_default());
    // include determinism knobs available in memory (if present in parent scope)
    // If you don't have 'CURRENT_RUN_CHANGE_IDS' in scope, fold it during run via a closure
    format!("sha256:{:x}", h.finalize())
}

Note: An even stronger approach is to salt input_sha with the current run.change_ids and build_id. The simplest way without refactoring is to wrap input_sha in the run loop and XOR a second hash made from spec.run.build_id + spec.run.change_ids. Example:

let mut base = input_sha(&spec.id, &step);
let mut h = Sha256::new();
h.update(base.as_bytes());
h.update(spec.run.build_id.clone().unwrap_or_default().as_bytes());
for cid in &spec.run.change_ids { h.update(cid.as_bytes()); }
base = format!("sha256:{:x}", h.finalize());
// use `base` as `input_sha`


⸻

6) Tiny doc & tests

Doc: Append to docs/WORKFLOWS_PLUGIN.md (under “Spec Schema”):

### Determinism & Evolution

- `run.build_id`: identifies the build (or compatibility set). Stored in visibility; useful for routing in Temporal.
- `run.change_ids`: list of change fences. The runner records this set in
  `.nexoscli/workflows/<id>/determinism.json` on first run and **fails** future runs if the set differs
  (use `--allow-change-ids-drift` to consciously update).

Check your spec first:

```bash
nexoscli workflow validate --spec docs/workflows/WF-demo.yaml --output json

**E2E test** (add under `nexoscli-bin/tests/workflow_change_ids.rs`):

```rust
use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;

#[test]
fn gate_on_change_ids_drift() {
    let spec1 = r#"version: "0.1"
id: WF-change-ids
run: { build_id: "build-A", change_ids: ["CH_STEP_ORDER_V1"] }
steps:
  - id: wait-50
    timer: { duration_ms: 50 }
"#;
    std::fs::create_dir_all(".nexoscli/workflows/WF-change-ids").unwrap();
    std::fs::write(".nexoscli/workflows/WF-change-ids/spec.yaml", spec1).unwrap();

    Command::cargo_bin("nexoscli").unwrap()
      .args(["workflow","run","--spec",".nexoscli/workflows/WF-change-ids/spec.yaml","--output","json"])
      .assert().success().stdout(contains("\"failed\":0"));

    // mutate change_ids
    let spec2 = r#"version: "0.1"
id: WF-change-ids
run: { build_id: "build-A", change_ids: ["CH_STEP_ORDER_V2"] }
steps:
  - id: wait-50
    timer: { duration_ms: 50 }
"#;
    std::fs::write(".nexoscli/workflows/WF-change-ids/spec.yaml", spec2).unwrap();

    // should fail without override
    Command::cargo_bin("nexoscli").unwrap()
      .args(["workflow","run","--spec",".nexoscli/workflows/WF-change-ids/spec.yaml","--output","json"])
      .assert().failure();

    // proceed with explicit override
    Command::cargo_bin("nexoscli").unwrap()
      .args(["workflow","run","--spec",".nexoscli/workflows/WF-change-ids/spec.yaml","--allow-change-ids-drift","--output","json"])
      .assert().success();
}


⸻

Smoke sequence

cargo build --workspace

# Validate a spec
./target/debug/nexoscli workflow validate --spec .nexoscli/workflows/WF-demo/spec.yaml --output json

# First run (pins determinism)
./target/debug/nexoscli workflow run --spec .nexoscli/workflows/WF-demo/spec.yaml --build-id build-A --output json
cat .nexoscli/workflows/WF-demo/determinism.json

# Edit change_ids in spec → should fail without override
# (or run the E2E test above)
cargo test --workspace


⸻

Acceptance criteria for T‑2 (what must be true)
	•	workflow validate catches schema errors, multi‑kind steps, bad resource names, invalid change IDs.
	•	First successful workflow run creates determinism.json with {build_id, change_ids, spec_sha256, first_run_at}.
	•	Subsequent runs fail on change_ids set drift unless --allow-change-ids-drift is passed, in which case determinism.json updates.
	•	Visibility includes build_id & used_change_ids.
	•	input_sha salting with build/change ids prevents false idempotent skips when determinism changes.

⸻

Why this step matters for Temporal

Temporal’s safe evolution hinges on workflow versioning (getVersion) and Build IDs for routing. With this drop:
	•	You have a conscious, enforced fence (change_ids) that agents/humans can bump via spec—no silent drift.
	•	You start pinning a Build ID per run, making it trivial to route later workers.
	•	Determinism metadata lives alongside history/effects and is packaged by TracePacks.

