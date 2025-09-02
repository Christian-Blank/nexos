use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;

mod helpers;

#[test]
fn gate_on_change_ids_drift_and_override() {
    let tmp = helpers::tmp();
    let root = tmp.path();

    // init temp workspace
    helpers::run_ok(root, &["dev", "init", "--yes"]);

    // First spec pins determinism (CH_V1)
    let spec1 = r#"version: "0.1"
id: WF-change-ids
run: { build_id: "build-A", change_ids: ["CH_V1"] }
steps:
  - id: wait-50
    timer: { duration_ms: 50 }
"#;
    std::fs::create_dir_all(root.join(".nexoscli/workflows/WF-change-ids")).unwrap();
    std::fs::write(root.join(".nexoscli/workflows/WF-change-ids/spec.yaml"), spec1).unwrap();

    // First run succeeds; failed == 0
    let first = helpers::run_json(
        root,
        &[
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-change-ids/spec.yaml",
        ],
    );
    assert_eq!(first["data"]["failed"].as_u64().unwrap_or(1), 0);

    // Change change_ids set to CH_V2 -> should fail without override
    let spec2 = r#"version: "0.1"
id: WF-change-ids
run: { build_id: "build-A", change_ids: ["CH_V2"] }
steps:
  - id: wait-50
    timer: { duration_ms: 50 }
"#;
    std::fs::write(root.join(".nexoscli/workflows/WF-change-ids/spec.yaml"), spec2).unwrap();

    // Run without override: expect non-zero exit and error envelope mentioning drift
    Command::cargo_bin("nexoscli")
        .unwrap()
        .current_dir(root)
        .args([
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-change-ids/spec.yaml",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stdout(contains("workflow.change_ids_drift"))
        .stdout(contains("change_ids_drift"));

    // Run with override flag: should now succeed
    Command::cargo_bin("nexoscli")
        .unwrap()
        .current_dir(root)
        .args([
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-change-ids/spec.yaml",
            "--allow-change-ids-drift",
            "--output",
            "json",
        ])
        .assert()
        .success();
}
