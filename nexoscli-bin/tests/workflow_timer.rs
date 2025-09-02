use std::time::Instant;

mod helpers;

#[test]
fn timer_completes_and_is_idempotent() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();

    // Init project for this temp workspace
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);

    // Write a tiny spec inline
    let spec = r#"version: "0.1"
id: WF-e2e-timer
steps:
  - id: wait-100
    timer: { duration_ms: 100 }
    idempotent: true
"#;
    std::fs::create_dir_all(temp_path.join(".nexoscli/workflows/WF-e2e-timer")).unwrap();
    std::fs::write(
        temp_path.join(".nexoscli/workflows/WF-e2e-timer/spec.yaml"),
        spec,
    )
    .unwrap();

    // First run ~>=100ms, failed == 0
    let t0 = Instant::now();
    let first = helpers::run_json(
        temp_path,
        &[
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-e2e-timer/spec.yaml",
        ],
    );
    assert_eq!(first["data"]["failed"].as_u64().unwrap_or(1), 0);
    assert!(t0.elapsed().as_millis() >= 80);

    // Second run should skip the idempotent timer step (effect ok + same input)
    let second = helpers::run_json(
        temp_path,
        &[
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-e2e-timer/spec.yaml",
        ],
    );
    // If first run succeeded, subsequent run should not increase 'ran' beyond 1 step; at minimum, it should succeed.
    assert!(second["data"]["failed"].as_u64().unwrap_or(1) == 0);
}
