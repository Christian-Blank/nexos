mod helpers;
use helpers::*;
use std::{fs, process::Command};

#[test]
fn workflow_temporal_smoke_outbox_created() {
    let t = tmp();

    // Initialize a basic git repo (parity with other tests)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    // Ensure workspace dirs exist
    run_ok(t.path(), &["dev", "init", "--yes"]);

    // Create a small spec with one activity and one tiny timer
    let spec_dir = t.path().join(".nexoscli/workflows/WF-smoke");
    fs::create_dir_all(&spec_dir).unwrap();
    let spec = spec_dir.join("spec.yaml");
    fs::write(&spec, r#"version: "0.1"
id: WF-smoke
steps:
  - id: a1
    activity: "dev.plugins.check"
    args: {}
    idempotent: true
    retry: { max_attempts: 1, backoff_ms: 50 }
    timeouts: {}
    resources: []
  - id: t1
    timer: { duration_ms: 5 }
"#).unwrap();

    // Emit envelopes (no execution)
    run_ok(t.path(), &["workflow","run","--spec", spec.to_str().unwrap(), "--engine","temporal"]);

    // Assert outbox exists and has at least start + 2 schedules
    let outbox = t.path().join(".nexoscli/engine/outbox");
    assert!(outbox.exists(), "outbox dir missing");
    let entries: Vec<_> = fs::read_dir(&outbox).unwrap().collect();
    assert!(entries.len() >= 3, "expected at least 3 envelopes, got {}", entries.len());
}
