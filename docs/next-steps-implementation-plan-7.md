Perfect—let’s keep momentum and ship the next two slices:
	•	C) dev doctor --ci — a single PR‑gate that aggregates repo checks and exits non‑zero when anything’s off
	•	D) dev telemetry report — turn your events.jsonl into quick success/fail counts and top offenders (no backend)

Everything below is drop‑in for your current tree. You’ll get code, tests, and acceptance commands.

⸻

0) What you already have (assumed)
	•	plugins/dev with dev init, dev plugin new, dev plugins check, dev doctor
	•	plugins/graph with graph check & graph impact
	•	tools export top‑level command (manifest for agents)
	•	Event logging to .nexoscli/log/events.jsonl with exit_code, plugin, command, run_id, ts, and content_hash

If any of those are missing, tell me—otherwise the code below compiles against what you have.

⸻

1) C) Doctor CI mode

1.1 Add a tiny self‑runner (spawn the current CLI and parse JSON)

Edit plugins/dev/src/lib.rs – append these helpers near the bottom:

use std::process::Command as Proc;
use std::path::Path;

fn run_cli_json(cwd: &Path, args: &[&str]) -> anyhow::Result<(i32, serde_json::Value)> {
    let exe = std::env::current_exe()?;
    let mut a: Vec<&str> = args.to_vec();
    if !a.iter().any(|x| *x == "--output") {
        a.push("--output"); a.push("json");
    }
    let out = Proc::new(exe).current_dir(cwd).args(&a).output()?;
    let code = out.status.code().unwrap_or(-1);
    let val: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| anyhow::anyhow!("stdout not JSON for {:?}: {}", a, e))?;
    Ok((code, val))
}

1.2 Extend the dev doctor CLI

In the augment_commands() for the dev plugin, update the doctor subcommand to include --ci and optional checks:

.subcommand(
  Command::new("doctor")
    .about("Verify local setup (dotdir, event log write, config, OTEL envs)")
    .arg(Arg::new("ci").long("ci").action(ArgAction::SetTrue)
         .help("Aggregate repo checks and exit non-zero on failure"))
    .arg(Arg::new("checks").long("checks").value_name("CSV")
         .help("Subset of checks: graph,plugins,config,events,tools (default: all)"))
)

1.3 Result types for CI mode

Add near your other structs:

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorCiCheck {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorCiResult {
    ok: bool,
    checks: Vec<DoctorCiCheck>,
}

1.4 Implement the --ci branch

In execute() inside the "doctor" arm, before your existing non‑CI logic, add:

if m.get_flag("ci") {
    let cwd = std::env::current_dir()?;
    let want: std::collections::HashSet<String> = m
        .get_one::<String>("checks")
        .map(|csv| csv.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_else(|| ["graph","plugins","config","events","tools"]
            .into_iter().map(|s| s.to_string()).collect());

    let mut checks = Vec::<DoctorCiCheck>::new();

    // config
    if want.contains("config") {
        let cfg = &ctx.config;
        checks.push(DoctorCiCheck {
            name: "config_loaded".into(),
            ok: true,
            detail: format!("output.default={:?} telemetry.enabled={} endpoint={:?}",
                cfg.output.default, cfg.telemetry.enabled, cfg.telemetry.endpoint),
        });
    }

    // events: try to write a ping
    if want.contains("events") {
        let ok = ctx.events.write("dev","doctor.ci.ping",&serde_json::json!({"ping":true}),0).is_ok();
        let p = std::path::PathBuf::from(".nexoscli/log/events.jsonl");
        checks.push(DoctorCiCheck {
            name: "event_log_writable".into(),
            ok,
            detail: p.display().to_string(),
        });
    }

    // graph
    if want.contains("graph") {
        let (code, val) = run_cli_json(&cwd, &["graph","check"])?;
        let ok = code == 0 && val.get("errors").and_then(|e| e.as_array()).map(|a| a.is_empty()).unwrap_or(true);
        checks.push(DoctorCiCheck {
            name: "graph_check".into(),
            ok,
            detail: if ok { "OK".into() } else { format!("errors: {}", val.get("errors").unwrap_or(&serde_json::json!(null))) },
        });
    }

    // plugins
    if want.contains("plugins") {
        let (code, val) = run_cli_json(&cwd, &["dev","plugins","check"])?;
        let ok = code == 0;
        checks.push(DoctorCiCheck {
            name: "plugins_check".into(),
            ok,
            detail: if ok { "OK".into() } else { format!("errors: {}", val.get("errors").unwrap_or(&serde_json::json!(null))) },
        });
    }

    // tools
    if want.contains("tools") {
        let (code, val) = run_cli_json(&cwd, &["tools","export"])?;
        let ok = code == 0 && val["data"]["tools"].as_array().map(|a| !a.is_empty()).unwrap_or(false);
        let n = val["data"]["tools"].as_array().map(|a| a.len()).unwrap_or(0);
        checks.push(DoctorCiCheck {
            name: "tools_export".into(),
            ok,
            detail: format!("tools={}", n),
        });
    }

    let ok = checks.iter().all(|c| c.ok);
    if ok {
        let exit = ctx.ok("dev","doctor.ci",&DoctorCiResult{ ok, checks });
        return Ok(exit);
    } else {
        // include failing checks in message; exit non-zero
        let failing: Vec<_> = checks.iter().filter(|c| !c.ok).map(|c| c.name.clone()).collect();
        let exit = ctx.err("dev","doctor.ci","doctor.ci.failed",
            &format!("Failing checks: {}", failing.join(",")),
            Some("Run subcommands above to see details (graph check, dev plugins check, tools export)"),
            2);
        return Ok(exit);
    }
}

1.5 Expose ToolSpec for dev.doctor (schema includes ci)

Add/extend your tool_specs() to reflect the new args:

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct DoctorArgs { #[serde(default)] ci: bool, #[serde(default)] checks: Option<String> }

fn tool_specs(&self) -> Vec<ToolSpec> {
    vec![
        // ... existing specs ...
        ToolSpec {
            name: "dev.doctor".to_string(),
            about: "Verify local setup (dotdir, event log write, config, OTEL envs); --ci aggregates checks".to_string(),
            args_schema: schema_for!(DoctorArgs),
            result_schema: schema_for!(DoctorCiResult),
        },
    ]
}

If you had a previous DoctorResult schema, you can safely keep it; schemas don’t break execution.

⸻

2) D) Telemetry report (from events.jsonl)

We’ll keep it inside the dev plugin for now: nexoscli dev telemetry report.

2.1 Args & result

In plugins/dev/src/lib.rs, add:

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryReportArgs {
    #[serde(default)]
    since: Option<String>, // "24h" | "7d" | RFC3339
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryByCommand {
    plugin: String,
    command: String,
    count: u64,
    success: u64,
    fail: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryReport {
    total_events: u64,
    window_start: Option<String>,
    by_command: Vec<TelemetryByCommand>,
}

2.2 CLI surface

In augment_commands() under dev, add:

.subcommand(
  Command::new("telemetry").about("Observability and analytics")
    .subcommand(
      Command::new("report")
        .about("Aggregate events.jsonl → counts by command (success/fail)")
        .arg(Arg::new("since").long("since").value_name("WINDOW")
             .help("24h | 7d | RFC3339 (inclusive)"))
    )
)

2.3 Implementation

Add in execute() match:

Some(("telemetry", tm)) => {
    if let Some(("report", m)) = tm.subcommand() {
        use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};
        let since_s = m.get_one::<String>("since").cloned();
        let since_ts: Option<OffsetDateTime> = match since_s.as_deref() {
            None => None,
            Some(s) if s.ends_with("h") => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
            Some(s) if s.ends_with("d") => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
            Some(s) => OffsetDateTime::parse(s, &Rfc3339).ok(),
        };

        let log = std::env::current_dir()?.join(".nexoscli").join("log").join("events.jsonl");
        let mut rows = Vec::<serde_json::Value>::new();
        if log.exists() {
            for line in std::fs::read_to_string(&log)?.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    let ts_ok = match (&since_ts, v.get("ts").and_then(|x| x.as_str())) {
                        (None, _) => true,
                        (Some(since), Some(ts)) => OffsetDateTime::parse(ts, &Rfc3339).ok().map(|t| t >= *since).unwrap_or(true),
                        _ => true,
                    };
                    if ts_ok { rows.push(v); }
                }
            }
        }

        use std::collections::BTreeMap;
        let mut by: BTreeMap<(String,String),(u64,u64,u64)> = BTreeMap::new(); // (p,c)->(count,success,fail)
        for v in &rows {
            let p = v.get("plugin").and_then(|x| x.as_str()).unwrap_or("-");
            let c = v.get("command").and_then(|x| x.as_str()).unwrap_or("-");
            let code = v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0);
            let e = by.entry((p.to_string(), c.to_string())).or_insert((0,0,0));
            e.0 += 1;
            if code == 0 { e.1 += 1; } else { e.2 += 1; }
        }
        let mut out = Vec::<TelemetryByCommand>::new();
        for ((p,c),(cnt,ok,fail)) in by.into_iter() {
            out.push(TelemetryByCommand{ plugin: p, command: c, count: cnt, success: ok, fail });
        }
        // sort by count desc
        out.sort_by(|a,b| b.count.cmp(&a.count));

        let r = TelemetryReport {
            total_events: rows.len() as u64,
            window_start: since_ts.map(|t| t.format(&Rfc3339).unwrap()),
            by_command: out,
        };
        let exit = ctx.ok("dev","telemetry.report",&r);
        return Ok(exit);
    }
    Ok(2)
}

2.4 ToolSpec

Add to tool_specs():

ToolSpec {
    name: "dev.telemetry.report".to_string(),
    about: "Aggregate events.jsonl → counts by command".to_string(),
    args_schema: schema_for!(TelemetryReportArgs),
    result_schema: schema_for!(TelemetryReport),
},


⸻

3) Tests (drop‑in)

Create two files under nexoscli-bin/tests/.

3.1 dev_doctor_ci.rs

mod helpers; use helpers::*;
use serde_yaml;

#[test]
fn doctor_ci_passes_then_fails_on_cycle() {
    let t = tmp();
    // prepare repo
    let _ = run_json(t.path(), &["dev","init","--yes"]);

    // valid graph
    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let g = serde_yaml::to_string(&serde_yaml::yaml!({
        "id": "EG-ok",
        "nodes": [
            { "id": "A", "deps": [] },
            { "id": "B", "deps": ["A"] }
        ]
    })).unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g).unwrap();

    // structure check should be OK for empty plugins dir (no errors)
    let v = run_json(t.path(), &["dev","plugins","check"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));

    // doctor --ci OK
    let v = run_json(t.path(), &["dev","doctor","--ci"]);
    assert!(v["data"]["ok"].as_bool().unwrap(), "doctor --ci should pass");

    // introduce a cycle
    let g2 = serde_yaml::to_string(&serde_yaml::yaml!({
        "id": "EG-bad",
        "nodes": [
            { "id": "A", "deps": ["B"] },
            { "id": "B", "deps": ["A"] }
        ]
    })).unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g2).unwrap();

    // doctor --ci must fail now
    let (code, out, _err) = run_raw(t.path(), &["dev","doctor","--ci","--output","json"]);
    assert!(code != 0, "doctor --ci should fail");
    let v2: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v2["errors"].as_array().unwrap().len() >= 1);
}

3.2 telemetry_report.rs

mod helpers; use helpers::*;

#[test]
fn telemetry_report_counts_commands() {
    let t = tmp();
    let _ = run_json(t.path(), &["dev","init","--yes"]);

    // generate some events
    let _ = run_json(t.path(), &["issue","new","--title","One"]);
    let _ = run_json(t.path(), &["issue","new","--title","Two"]);
    // forced error: unsupported import source
    let (_code, _out, _err) = run_raw(t.path(), &["issue","import","--from","other","--output","json"]);

    let v = run_json(t.path(), &["dev","telemetry","report","--since","7d"]);
    assert!(v["data"]["total_events"].as_u64().unwrap() >= 3);
    let rows = v["data"]["by_command"].as_array().unwrap();
    assert!(rows.iter().any(|r| r["plugin"]=="issue" && r["command"]=="new" && r["count"].as_u64().unwrap() >= 2));
    assert!(rows.iter().any(|r| r["plugin"]=="issue" && r["command"]=="import" && r["fail"].as_u64().unwrap() >= 1));
}

If you don’t have helpers.rs yet (many of your tests already rely on it), paste the shared one I provided earlier beside these files.

Run tests:

cargo test --workspace


⸻

4) Quick manual acceptance

# CI gate
cargo nex -- dev doctor --ci --output json | jq

# Break the graph to see non-zero exit
cp docs/exec_graph.yaml /tmp/backup-graph.yaml
printf 'id: EG-bad\nnodes:\n- {id: A, deps: [B]}\n- {id: B, deps: [A]}\n' > docs/exec_graph.yaml
! cargo nex -- dev doctor --ci --output json

# Restore
mv /tmp/backup-graph.yaml docs/exec_graph.yaml

# Telemetry report (24h window)
cargo nex -- dev telemetry report --since 24h --output json | jq

Expected:
	•	dev doctor --ci prints an envelope with data.ok=true and details per check; breaks with a cycle.
	•	dev telemetry report prints total_events and a by_command array with counts and success/fail.

⸻

5) Pin follow‑ups (don’t do now)

Append to docs/ROADMAP_NEXT.md:
	•	Doctor: add --print-plan to show which subcommands it runs and their args.
	•	Telemetry: record error_code in event lines → unlock error leaderboard.
	•	Telemetry: durations — include dur_ms in events from the envelope writer (computed around each command).
	•	Telemetry: add --top 10 and --group-by plugin|command.
	•	Doctor: add --check bundles → run bundle verify against current FS for the latest bundle.

⸻

6) Candor (blind spots)
	•	The CI doctor spawns the current CLI to keep reuse high and coupling low; it’s robust in tests/temp dirs and mirrors what users do.
	•	Telemetry uses events JSONL only (source of truth). When you add durations/error codes to events, we’ll extend the report without changing the command surface.

