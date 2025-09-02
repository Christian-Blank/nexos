mod helpers;
use helpers::*;
use std::{fs, process::Command};

#[test]
fn workflow_run_temporal_emits_outbox() {
    let t = tmp();

    // Initialize a basic git repo (keeps parity with other tests)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    // Ensure workspace dirs exist
    run_ok(t.path(), &["dev", "init", "--yes"]);

    // Create a simple spec with an activity and a timer
    let spec_id = "WF-temporal";
    let spec_dir = t.path().join(".nexoscli/workflows").join(spec_id);
    fs::create_dir_all(&spec_dir).unwrap();
    let spec_path = spec_dir.join("spec.yaml");

    let spec_yaml = r#"version: "0.1"
id: WF-temporal
run:
  build_id: "build-T1"
  change_ids: ["CH_T1"]
  memo: {}
  search_attributes: {}
steps:
  - id: a1
    activity: "dev.plugins.check"
    args: {}
    idempotent: true
    retry: { max_attempts: 1, backoff_ms: 100 }
    timeouts: {}
    resources: []
  - id: t1
    timer: { duration_ms: 10 }
    args: {}
    idempotent: true
    retry: { max_attempts: 1, backoff_ms: 100 }
    timeouts: {}
    resources: []
"#;
    fs::write(&spec_path, spec_yaml).unwrap();

    // Run with temporal engine
    let j = run_json(
        t.path(),
        &[
            "workflow","run",
            "--spec", spec_path.to_str().unwrap(),
            "--engine", "temporal",
        ],
    );

    // Assert scheduled == 2
    let scheduled = j["data"]["scheduled"].as_u64().unwrap_or(0);
    assert_eq!(scheduled, 2, "expected 2 scheduled steps, got {}", scheduled);

    // Outbox should contain at least 3 envelopes (start + 2 schedules)
    let outbox = t.path().join(".nexoscli/engine/outbox");
    assert!(outbox.exists(), "outbox directory not created at {:?}", outbox);
    let entries: Vec<_> = fs::read_dir(&outbox).unwrap().collect();
    assert!(entries.len() >= 3, "expected at least 3 outbox envelopes, found {}", entries.len());

    // Check for required operations and common fields
    let mut has_start = false;
    let mut has_activity_schedule = false;
    let mut has_timer_schedule = false;

    for entry in entries {
        let path = entry.unwrap().path();
        let content = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();

        // Common fields
        assert!(v.get("op").is_some(), "missing op in {:?}", path);
        assert!(v.get("workflow").is_some(), "missing workflow in {:?}", path);
        assert!(v.get("key").is_some(), "missing key in {:?}", path);
        assert!(v.get("attrs").is_some(), "missing attrs in {:?}", path);
        assert!(v.get("body").is_some(), "missing body in {:?}", path);
        assert!(v.get("ts").is_some(), "missing ts in {:?}", path);

        // Specific checks
        let op = v.get("op").unwrap().as_str().unwrap();
        match op {
            "start" => {
                has_start = true;
                assert_eq!(v.get("workflow").unwrap().as_str().unwrap(), spec_id);
                assert_eq!(v.get("key").unwrap().as_str().unwrap(), spec_id);
            }
            "activity.schedule" => { has_activity_schedule = true; }
            "timer.schedule" => { has_timer_schedule = true; }
            _ => {}
        }
    }

    assert!(has_start, "expected start envelope");
    assert!(has_activity_schedule, "expected activity.schedule envelope");
    assert!(has_timer_schedule, "expected timer.schedule envelope");
}

#[test]
fn workflow_signal_temporal_writes_signal() {
    let t = tmp();

    // Initialize a basic git repo (parity)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    // Ensure workspace dirs exist
    run_ok(t.path(), &["dev", "init", "--yes"]);

    // Emit a temporal signal by id (no spec required)
    let wf_id = "WF-signal";
    run_ok(
        t.path(),
        &[
            "workflow","signal",
            "--id", wf_id,
            "--name", "pause",
            "--engine", "temporal",
        ],
    );

    // Outbox must contain a "signal" op with name "pause"
    let outbox = t.path().join(".nexoscli/engine/outbox");
    assert!(outbox.exists(), "outbox directory not created at {:?}", outbox);
    let entries: Vec<_> = fs::read_dir(&outbox).unwrap().collect();
    assert!(!entries.is_empty(), "expected at least one outbox envelope");

    let mut found = false;
    for entry in entries {
        let path = entry.unwrap().path();
        let content = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        if v.get("op").and_then(|x| x.as_str()) == Some("signal")
            && v.get("workflow").and_then(|x| x.as_str()) == Some(wf_id)
            && v.get("name").and_then(|x| x.as_str()) == Some("pause") {
            // Basic shape present
            assert!(v.get("key").is_some());
            assert!(v.get("attrs").is_some());
            assert!(v.get("body").is_some());
            assert!(v.get("ts").is_some());
            found = true;
            break;
        }
    }
    assert!(found, "expected a signal envelope for workflow {}, name 'pause'", wf_id);
}
