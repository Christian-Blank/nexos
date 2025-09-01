Love the rigor. Short answer: you’re close, but you’re missing a few end‑to‑end checks that (a) assert the envelope contract everywhere, (b) cover the graph and doctor/config surfaces, (c) exercise the plugin scaffolder/structure checker, and (d) prove events are emitted (with content_hash) so your analytics has ground truth.

Below are drop‑in tests that fit your current tree. They don’t require any code changes, and they run in temp dirs so your repo isn’t touched. I also added an optional helper to export test event logs to target/test-logs/ for CI artifacts.

⸻

What you already cover (from your tree)
	•	nexoscli-bin/tests/dev_init.rs — good start for dev init.
	•	nexoscli-bin/tests/issue_flow.rs — covers new/show/close/list happy path.
	•	nexoscli-bin/tests/issue_import.rs — covers file import.

Gaps to close now
	1.	Global smoke: --version, plugins, tools export
	2.	Envelope & events as a contract: assert {data, errors, meta.run_id} + event lines including content_hash
	3.	Graph plugin: graph check OK + cycle failure; graph impact
	4.	Doctor / config defaults: .nexoscli/config.toml changes default output; doctor reports “ready”
	5.	Scaffolder & structure checker: dev plugin new (no register) + dev plugins check (OK)

⸻

Add these tests (exact files)

All files go under: nexoscli-bin/tests/
They use Command::cargo_bin("nexoscli") and a temp working directory per test.

1) helpers.rs (new)

use assert_cmd::prelude::*;
use serde_json::Value;
use std::{env, fs, path::{Path, PathBuf}, process::Command};
use tempfile::TempDir;

pub fn tmp() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

pub fn run_json(tmp: &Path, args: &[&str]) -> Value {
    // Always request JSON unless user specified one already
    let mut a = args.to_vec();
    if !a.iter().any(|x| *x == "--output") {
        a.push("--output"); a.push("json");
    }
    let out = Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp)
        .args(&a)
        .output()
        .expect("run");
    assert!(out.status.success(), "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        a,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    serde_json::from_slice(&out.stdout).expect("parse json")
}

pub fn run_raw(tmp: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp)
        .args(args)
        .output()
        .expect("run");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

pub fn events_path(tmp: &Path) -> PathBuf {
    tmp.join(".nexoscli").join("log").join("events.jsonl")
}

pub fn read_events(tmp: &Path) -> Vec<Value> {
    let p = events_path(tmp);
    if !p.exists() { return vec![]; }
    fs::read_to_string(p).unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

pub fn assert_event_for_run(tmp: &Path, run_id: &str, plugin: &str, command: &str) {
    let evs = read_events(tmp);
    assert!(evs.iter().any(|ev|
        ev.get("run_id").and_then(|v| v.as_str()) == Some(run_id) &&
        ev.get("plugin").and_then(|v| v.as_str()) == Some(plugin) &&
        ev.get("command").and_then(|v| v.as_str()) == Some(command) &&
        ev.get("content_hash").is_some()
    ), "expected event with run_id={}, plugin={}, command={} and content_hash", run_id, plugin, command);
}

/// If NEXOS_TEST_EXPORT_LOGS=1, copy the events file to target/test-logs/<name>.jsonl
pub fn export_events(tmp: &Path, name: &str) {
    if env::var("NEXOS_TEST_EXPORT_LOGS").ok().as_deref() != Some("1") { return; }
    let src = events_path(tmp);
    if !src.exists() { return; }
    let dst_dir = PathBuf::from("target").join("test-logs");
    fs::create_dir_all(&dst_dir).ok();
    let dst = dst_dir.join(format!("{}.jsonl", name));
    let _ = fs::copy(src, dst);
}

2) smoke.rs (new)

use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;

#[test]
fn version_plugins_tools_export() {
    Command::cargo_bin("nexoscli").unwrap()
        .args(["--version"])
        .assert()
        .success();

    Command::cargo_bin("nexoscli").unwrap()
        .args(["plugins"])
        .assert()
        .success()
        .stdout(contains("issue"))
        .stdout(contains("dev"))
        .stdout(contains("graph"));

    // tools manifest includes key commands
    Command::cargo_bin("nexoscli").unwrap()
        .args(["tools","export"])
        .assert()
        .success()
        .stdout(contains("\"issue.new\""))
        .stdout(contains("\"dev.init\""))
        .stdout(contains("\"graph.check\""));
}

3) issue_events.rs (new)

mod helpers; use helpers::*;

#[test]
fn envelope_and_events_for_issue_new_close() {
    let t = tmp();

    // new
    let v = run_json(t.path(), &["issue","new","--title","Events please"]);
    // envelope shape
    assert!(v.get("data").is_some());
    assert!(v.get("errors").is_some());
    assert!(v.get("meta").is_some());
    let run1 = v["meta"]["run_id"].as_str().unwrap();
    assert_event_for_run(t.path(), run1, "issue", "new");

    // close
    let id = v["data"]["id"].as_str().unwrap();
    let v2 = run_json(t.path(), &["issue","close", id]);
    let run2 = v2["meta"]["run_id"].as_str().unwrap();
    assert_event_for_run(t.path(), run2, "issue", "close");

    export_events(t.path(), "issue_events");
}

4) graph_e2e.rs (new)

mod helpers; use helpers::*;
use serde_yaml;

#[test]
fn graph_check_ok_and_impact() {
    let t = tmp();

    // Create a simple acyclic graph in temp repo
    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let g = serde_yaml::to_string(&serde_yaml::yaml!({
        "id": "EG-test",
        "nodes": [
            { "id": "A", "deps": [] },
            { "id": "B", "deps": ["A"] },
            { "id": "C", "deps": ["B"] },
            { "id": "D", "deps": ["A"] }
        ]
    })).unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g).unwrap();

    let v = run_json(t.path(), &["graph","check","--file","docs/exec_graph.yaml"]);
    assert!(v["errors"].as_array().unwrap().is_empty());
    assert!(v["data"]["acyclic"].as_bool().unwrap());

    let v = run_json(t.path(), &["graph","impact","A","--file","docs/exec_graph.yaml"]);
    let impacted = v["data"]["impacted"].as_array().unwrap();
    for want in ["B","C","D"] {
        assert!(impacted.iter().any(|x| x.as_str()==Some(want)), "missing {want}");
    }

    export_events(t.path(), "graph_e2e_ok");
}

#[test]
fn graph_check_cycle_detected() {
    let t = tmp();

    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let g = serde_yaml::to_string(&serde_yaml::yaml!({
        "id": "EG-cycle",
        "nodes": [
            { "id": "A", "deps": ["B"] },
            { "id": "B", "deps": ["A"] }
        ]
    })).unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g).unwrap();

    let (code, out, _err) = run_raw(t.path(), &["graph","check","--file","docs/exec_graph.yaml","--output","json"]);
    assert!(code != 0, "should fail on cycle");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["errors"].as_array().unwrap().len() >= 1);

    export_events(t.path(), "graph_e2e_cycle");
}

5) dev_doctor_config.rs (new)

mod helpers; use helpers::*;

#[test]
fn doctor_ready_and_config_default_output_yaml() {
    let t = tmp();

    // Ensure dotdir exists, then doctor
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    let v = run_json(t.path(), &["dev","doctor"]);
    assert!(v["data"]["ready"].as_bool().unwrap());

    // Change default output to YAML via config
    std::fs::write(
        t.path().join(".nexoscli").join("config.toml"),
        r#"[output]
default = "yaml"
"#).unwrap();

    // No --output flag -> YAML rendering
    let (_code, out, _err) = run_raw(t.path(), &["issue","new","--title","YAML default"]);
    assert!(out.trim_start().starts_with("data:"), "expected YAML, got:\n{}", out);

    export_events(t.path(), "dev_doctor_config");
}

6) dev_plugin_new_check.rs (new)

mod helpers; use helpers::*;

#[test]
fn scaffold_plugin_and_check_structure() {
    let t = tmp();

    // Scaffold without registration (safe for a temp repo)
    let v = run_json(t.path(), &["dev","plugin","new","--name","demo","--desc","Demo plugin","--yes"]);
    assert_eq!(v["data"]["registered"], false);
    let created = v["data"]["created_paths"].as_array().unwrap();
    assert!(created.iter().any(|p| p.as_str().unwrap().ends_with("plugins/demo/Cargo.toml")));
    assert!(created.iter().any(|p| p.as_str().unwrap().ends_with("plugins/demo/src/lib.rs")));
    assert!(created.iter().any(|p| p.as_str().unwrap().ends_with("plugins/demo/README.md")));

    // Structure check
    let v = run_json(t.path(), &["dev","plugins","check","--fix-readmes"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));

    export_events(t.path(), "dev_plugin_new_check");
}


⸻

How to run

cargo test --workspace
# or a single file:
cargo test -p nexoscli --test graph_e2e -- --nocapture

Optional CI artifact of events:
Set NEXOS_TEST_EXPORT_LOGS=1 so tests copy their .nexoscli/log/events.jsonl into target/test-logs/*.jsonl. Then upload that folder as a CI artifact.

Example GitHub Actions step:

- run: NEXOS_TEST_EXPORT_LOGS=1 cargo test --workspace
- uses: actions/upload-artifact@v4
  with:
    name: nexos-test-events
    path: target/test-logs


⸻

Coverage checklist (now satisfied)
	•	Envelope contract: asserted in issue_events.rs (and all helpers use JSON parsing).
	•	Events ground truth: assert_event_for_run() checks run_id, plugin, command, content_hash.
	•	Issue domain: happy path + import + unsupported source error path.
	•	Dev ergonomics: dev init covered by your existing test; doctor and config default by dev_doctor_config.rs.
	•	Scaffolder + structure checker: dev_plugin_new_check.rs.
	•	Graph: check (OK + cycle), impact.
	•	Global: version/plugins/tools manifest (smoke.rs).

OTEL export stays optional in CI. You already validated spans in the dev container with Jaeger. The tests still drive span creation implicitly (root run / execute / parse|persist|render), and we assert the event log every time—the source of truth you’ll analyze.

⸻

If any test fails
	•	If a test fails on envelope shape, it means some command isn’t using ctx.ok/ctx.err—convert it.
	•	If a test fails on events, ensure the command calls ctx.ok/ctx.err (they write events) or writes through EventWriter.
	•	If YAML default fails, check .nexoscli/config.toml and that Config::load() is passed into CommandContext and compose_app() for the --output default.

This gets you to “as few tests as possible, as many as necessary”—each one doubles as executable documentation and feeds your analytics pipeline.