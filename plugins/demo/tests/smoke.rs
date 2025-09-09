use std::process::Command;

#[test]
fn demo_hello_defaults_to_world_json_envelope() {
    // Run the workspace binary and invoke the demo hello command
    let output = Command::new("cargo")
        .args(["run", "-p", "nexoscli", "--", "demo", "hello", "--output", "json"])
        .output()
        .expect("failed to run nexoscli");

    assert!(
        output.status.success(),
        "non-zero exit: status={:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Basic envelope shape checks
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("\"message\""),
        "missing message field in envelope. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("hello, world"),
        "unexpected message content. stdout:\n{}",
        stdout
    );
}

#[test]
fn demo_hello_with_name_param() {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "nexoscli",
            "--",
            "demo",
            "hello",
            "--name",
            "Alice",
            "--output",
            "json",
        ])
        .output()
        .expect("failed to run nexoscli");

    assert!(
        output.status.success(),
        "non-zero exit: status={:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello, Alice"),
        "unexpected message content for --name. stdout:\n{}",
        stdout
    );
}
