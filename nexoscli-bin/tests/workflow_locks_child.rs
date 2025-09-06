use assert_cmd::prelude::*;
use std::{process::Command, thread, time::Duration};

mod helpers;

#[test]
fn lock_serializes_and_child_runs() {
    let tmp = helpers::tmp();
    let tmp_path = tmp.path();

    // init temp workspace
    helpers::run_ok(tmp_path, &["dev", "init", "--yes"]);

    // lock holder: grabs "git" for 1500ms
    let holder = r#"version: "0.1"
id: WF-lock-holder
steps:
  - id: hold
    timer: { duration_ms: 1500 }
    resources: ["git"]
    lock_timeout_ms: 2000
    idempotent: false
"#;
    std::fs::create_dir_all(tmp_path.join(".nexoscli/workflows/WF-lock-holder")).unwrap();
    std::fs::write(tmp_path.join(".nexoscli/workflows/WF-lock-holder/spec.yaml"), holder).unwrap();

    // contender: short timeout on same resource
    let contender = r#"version: "0.1"
id: WF-lock-contender
steps:
  - id: need
    timer: { duration_ms: 10 }
    resources: ["git"]
    lock_timeout_ms: 300
    idempotent: false
"#;
    std::fs::create_dir_all(tmp_path.join(".nexoscli/workflows/WF-lock-contender")).unwrap();
    std::fs::write(tmp_path.join(".nexoscli/workflows/WF-lock-contender/spec.yaml"), contender).unwrap();

    // run holder in background
    let mut bg = Command::cargo_bin("nexoscli").unwrap();
    bg.current_dir(tmp_path)
        .args([
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-lock-holder/spec.yaml",
            "--output",
            "json",
        ]);
    let mut child = bg.spawn().expect("spawn holder");

    // small delay to ensure lock acquired
    thread::sleep(Duration::from_millis(200));

    // contender should record a timeout (failed >= 1) even if process exit is 0
    let j = helpers::run_json(
        tmp_path,
        &[
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-lock-contender/spec.yaml",
        ],
    );
    assert!(j["data"]["failed"].as_u64().unwrap_or(0) >= 1);

    let _ = child.wait();
}

#[test]
fn child_wait_true_propagates() {
    let tmp = helpers::tmp();
    let tmp_path = tmp.path();

    // init temp workspace
    helpers::run_ok(tmp_path, &["dev", "init", "--yes"]);

    // child
    let child = r#"version: "0.1"
id: WF-child-e2e
steps:
  - id: wait
    timer: { duration_ms: 50 }
"#;
    std::fs::create_dir_all(tmp_path.join("examples/workflows")).unwrap();
    std::fs::write(tmp_path.join("examples/workflows/WF-child.yaml"), child).unwrap();

    // parent
    let parent = r#"version: "0.1"
id: WF-parent-e2e
run:
  search_attributes: { owner: "christian" }
steps:
  - id: call-child
    child_workflow:
      spec: examples/workflows/WF-child.yaml
      wait: true
      inherit_search_attributes: true
"#;
    std::fs::write(tmp_path.join("examples/workflows/WF-parent.yaml"), parent).unwrap();

    // parent should succeed; failed == 0
    let parent_out = helpers::run_json(
        tmp_path,
        &["workflow","run","--spec","examples/workflows/WF-parent.yaml"],
    );
    assert_eq!(parent_out["data"]["failed"].as_u64().unwrap_or(1), 0);

    // child should exist and have status
    Command::cargo_bin("nexoscli")
        .unwrap()
        .current_dir(tmp_path)
        .args(["workflow", "status", "--id", "WF-child-e2e", "--output", "json"])
        .assert()
        .success();
}
