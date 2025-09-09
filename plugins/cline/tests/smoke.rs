use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn mktemp() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut p = std::env::temp_dir();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock went backwards")
        .as_micros();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    p.push(format!("nexos_cline_smoke_{}_{}", ts, n));
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
fn cline_locate_json_envelope() {
    let t = mktemp();

    let out = Command::new(bin_path())
        .args(["cline", "locate", "--output", "json"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli cline locate");
    assert!(
        out.status.success(),
        "locate non-zero: status={:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
}

#[test]
fn cline_import_dry_run_with_fixture() {
    let t = mktemp();

    // Prepare a minimal tasks dir fixture
    let tasks_dir = t.join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    let sample = serde_json::json!({
        "id": "T-1",
        "title": "sample task",
        "messages": [{"role":"user","content":"hi"}]
    });
    fs::write(tasks_dir.join("task1.json"), serde_json::to_vec_pretty(&sample).unwrap()).unwrap();

    let out = Command::new(bin_path())
        .args([
            "cline",
            "import",
            "--storage",
            tasks_dir.to_string_lossy().as_ref(),
            "--dry-run",
            "--output",
            "json",
        ])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli cline import");
    assert!(
        out.status.success(),
        "import non-zero: status={:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"data\"") && (stdout.contains("\"tasks\"") || stdout.contains("\"imported\"")),
        "missing data/tasks in envelope. stdout:\n{}",
        stdout
    );
}

#[test]
fn cline_correlate_dry_run_summary() {
    let t = mktemp();

    let out = Command::new(bin_path())
        .args(["cline", "correlate", "--dry-run", "--pad", "5m", "--output", "json"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli cline correlate");
    assert!(
        out.status.success(),
        "correlate non-zero: status={:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Envelope basics; tolerate empty summary
    assert!(
        stdout.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout
    );
}
