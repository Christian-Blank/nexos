use std::process::Command;

#[test]
fn graph_check_defaults_json_envelope() {
    let file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/exec_graph.yaml");
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "nexoscli",
            "--",
            "graph",
            "check",
            "--output",
            "json",
        ])
        .arg("--file")
        .arg(file.to_string_lossy().to_string())
        .output()
        .expect("failed to run nexoscli graph check");

    assert!(
        output.status.success(),
        "non-zero exit: status={:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Basic envelope/data checks
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
    // CheckSummary shape hints
    assert!(
        stdout.contains("\"acyclic\""),
        "missing acyclic in output. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("\"unique_ids\""),
        "missing unique_ids in output. stdout:\n{}",
        stdout
    );
}

#[test]
fn graph_impact_returns_impacted_list() {
    let file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/exec_graph.yaml");
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "nexoscli",
            "--",
            "graph",
            "impact",
            "F0.1-nexoscli-issue-local",
            "--output",
            "json",
        ])
        .arg("--file")
        .arg(file.to_string_lossy().to_string())
        .output()
        .expect("failed to run nexoscli graph impact");

    assert!(
        output.status.success(),
        "non-zero exit: status={:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Basic envelope/data checks
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
    // Result shape checks
    assert!(
        stdout.contains("\"id\"") && stdout.contains("\"impacted\""),
        "missing id/impacted fields. stdout:\n{}",
        stdout
    );
}
