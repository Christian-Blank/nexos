use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, Ordering};
use serde_json;

fn mktemp() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut p = std::env::temp_dir();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_micros(); // finer granularity than millis
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    p.push(format!("nexos_rules_smoke_{}_{}", ts, n));
    fs::create_dir_all(&p).expect("failed to create temp dir");
    p
}

fn bin_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../target/debug");
    let exe = if cfg!(target_os = "windows") { "nexoscli.exe" } else { "nexoscli" };
    p.push(exe);
    p
}

#[test]
fn rules_lint_ok_after_init_json_envelope() {
    let t = mktemp();

    // rules init --force
    let init = Command::new(bin_path())
        .args(["rules", "init", "--force"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli rules init");
    assert!(
        init.status.success(),
        "init non-zero exit: status={:?}\nstderr:\n{}",
        init.status.code(),
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(
        t.join("rules/pack/core/RULE-use-term-run.md").exists(),
        "expected rule file not created by init"
    );

    // rules lint --output json
    let lint = Command::new(bin_path())
        .args(["rules", "lint", "--output", "json"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli rules lint");
    assert!(
        lint.status.success(),
        "lint non-zero exit: status={:?}\nstderr:\n{}",
        lint.status.code(),
        String::from_utf8_lossy(&lint.stderr)
    );

    let stdout = String::from_utf8_lossy(&lint.stdout);
    // Envelope basics
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
    // Result shape checks
    assert!(
        stdout.contains("\"files\""),
        "missing files field in data. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("\"issues\""),
        "missing issues field in data. stdout:\n{}",
        stdout
    );
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout JSON");
    assert!(
        v.get("data").and_then(|d| d.get("ok")).and_then(|b| b.as_bool()) == Some(true),
        "expected data.ok=true after init pack. stdout:\n{}",
        stdout
    );
}

#[test]
fn rules_export_human_creates_output_and_envelope() {
    let t = mktemp();

    // rules init --force
    let init = Command::new(bin_path())
        .args(["rules", "init", "--force"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli rules init");
    assert!(
        init.status.success(),
        "init non-zero exit: status={:?}\nstderr:\n{}",
        init.status.code(),
        String::from_utf8_lossy(&init.stderr)
    );

    // rules export --audience human --out docs/RULES.md --output json
    let export = Command::new(bin_path())
        .args([
            "rules",
            "export",
            "--audience",
            "human",
            "--out",
            "docs/RULES.md",
            "--output",
            "json",
        ])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli rules export");
    assert!(
        export.status.success(),
        "export non-zero exit: status={:?}\nstderr:\n{}",
        export.status.code(),
        String::from_utf8_lossy(&export.stderr)
    );

    let stdout = String::from_utf8_lossy(&export.stdout);
    // Envelope basics
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains("\"audience\"") && stdout.contains("\"out\""),
        "missing audience/out fields in data. stdout:\n{}",
        stdout
    );

    // Output file checks
    let out_path = t.join("docs/RULES.md");
    assert!(
        out_path.exists(),
        "expected export output file not found at {}",
        out_path.display()
    );
    let body = fs::read_to_string(&out_path).expect("failed to read exported file");
    assert!(
        body.contains("RULE-use-term-run") || body.contains("# Rules for human"),
        "unexpected export content. body:\n{}",
        body
    );
}
