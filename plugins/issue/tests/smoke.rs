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
    p.push(format!("nexos_issue_smoke_{}_{}", ts, n));
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
fn issue_flow_new_list_show_close_json_envelopes() {
    let t = mktemp();

    // issue new
    let new = Command::new(bin_path())
        .args([
            "issue",
            "new",
            "-t",
            "First issue",
            "--labels",
            "bug,help wanted",
            "--output",
            "json",
        ])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli issue new");
    assert!(
        new.status.success(),
        "new non-zero exit: status={:?}\nstderr:\n{}",
        new.status.code(),
        String::from_utf8_lossy(&new.stderr)
    );
    let stdout_new = String::from_utf8_lossy(&new.stdout);
    assert!(
        stdout_new.contains("\"data\""),
        "missing data field in envelope. stdout:\n{}",
        stdout_new
    );
    let v_new: serde_json::Value =
        serde_json::from_str(stdout_new.trim()).expect("stdout JSON (issue new)");
    let id = v_new
        .get("data")
        .and_then(|d| d.get("id"))
        .and_then(|s| s.as_str())
        .expect("data.id present")
        .to_string();

    // issue list
    let list = Command::new(bin_path())
        .args(["issue", "list", "--state", "all", "--output", "json"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli issue list");
    assert!(
        list.status.success(),
        "list non-zero exit: status={:?}\nstderr:\n{}",
        list.status.code(),
        String::from_utf8_lossy(&list.stderr)
    );
    let stdout_list = String::from_utf8_lossy(&list.stdout);
    assert!(
        stdout_list.contains("\"data\"") && stdout_list.contains("\"items\""),
        "missing data/items in envelope. stdout:\n{}",
        stdout_list
    );
    let v_list: serde_json::Value =
        serde_json::from_str(stdout_list.trim()).expect("stdout JSON (issue list)");
    let items = v_list
        .get("data")
        .and_then(|d| d.get("items"))
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !items.is_empty(),
        "expected at least one issue in list after new. stdout:\n{}",
        stdout_list
    );

    // issue show
    let show = Command::new(bin_path())
        .args(["issue", "show", &id, "--output", "json"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli issue show");
    assert!(
        show.status.success(),
        "show non-zero exit: status={:?}\nstderr:\n{}",
        show.status.code(),
        String::from_utf8_lossy(&show.stderr)
    );
    let stdout_show = String::from_utf8_lossy(&show.stdout);
    assert!(
        stdout_show.contains(&format!("\"{}\"", id)),
        "show response missing id. stdout:\n{}",
        stdout_show
    );

    // issue close
    let close = Command::new(bin_path())
        .args(["issue", "close", &id, "--output", "json"])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli issue close");
    assert!(
        close.status.success(),
        "close non-zero exit: status={:?}\nstderr:\n{}",
        close.status.code(),
        String::from_utf8_lossy(&close.stderr)
    );
    let stdout_close = String::from_utf8_lossy(&close.stdout);
    let v_close: serde_json::Value =
        serde_json::from_str(stdout_close.trim()).expect("stdout JSON (issue close)");
    assert!(
        v_close
            .get("data")
            .and_then(|d| d.get("state"))
            .and_then(|s| s.as_str())
            == Some("closed"),
        "close response missing state=closed. stdout:\n{}",
        stdout_close
    );
}

#[test]
fn issue_import_file_creates_issues_and_envelope() {
    let t = mktemp();

    // prepare fixture
    let arr = serde_json::json!([
        { "title": "A bug", "body": "desc", "labels": ["bug"] },
        { "title": "B task", "labels": "help wanted,good first issue" }
    ]);
    fs::create_dir_all(t.join("fixtures")).unwrap();
    let path = t.join("fixtures/issues.json");
    fs::write(&path, serde_json::to_vec_pretty(&arr).unwrap()).unwrap();

    // import
    let import = Command::new(bin_path())
        .args([
            "issue",
            "import",
            "--from",
            "file",
            "--path",
            path.to_string_lossy().as_ref(),
            "--output",
            "json",
        ])
        .current_dir(&t)
        .output()
        .expect("failed to run nexoscli issue import");
    assert!(
        import.status.success(),
        "import non-zero exit: status={:?}\nstderr:\n{}",
        import.status.code(),
        String::from_utf8_lossy(&import.stderr)
    );
    let stdout = String::from_utf8_lossy(&import.stdout);
    assert!(
        stdout.contains("\"data\"") && stdout.contains("\"created\""),
        "missing data/created in envelope. stdout:\n{}",
        stdout
    );
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout JSON (issue import)");
    let created_len = v
        .get("data")
        .and_then(|d| d.get("created"))
        .and_then(|a| a.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(created_len == 2, "expected 2 created, got {}. stdout:\n{}", created_len, stdout);

    // spot check that files exist
    let issues_dir = t.join(".nexoscli/issues");
    assert!(
        issues_dir.exists(),
        "expected issues dir to exist at {}",
        issues_dir.display()
    );
}
