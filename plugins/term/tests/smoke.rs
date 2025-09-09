use std::path::PathBuf;
use std::process::Command;

/// Resolve built nexoscli binary path for invoking in temp dirs without cargo workspace context
fn bin_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // plugins/term -> target/debug/nexoscli
    p.push("../../target/debug");
    let exe = if cfg!(target_os = "windows") { "nexoscli.exe" } else { "nexoscli" };
    p.push(exe);
    p
}

#[test]
fn term_run_echo_hello_json_envelope() {
    // Run a harmless command and assert envelope and result fields
    // We rely on cmd/sh shell via the plugin to execute 'echo hello'
    let exe = bin_path();
    let out = Command::new(exe)
        .args([
            "term",
            "run",
            "echo",
            "hello",
            "--output",
            "json",
        ])
        .output()
        .expect("failed to execute nexoscli term run");

    assert!(
        out.status.success(),
        "non-zero exit: status={:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    // Basic envelope shape
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("\"cmd\"") && stdout.contains("\"cwd\""),
        "missing cmd/cwd fields in data. stdout:\n{}",
        stdout
    );
    // The sample should include 'hello'
    assert!(
        stdout.contains("hello"),
        "expected 'hello' in output sample. stdout:\n{}",
        stdout
    );
}
