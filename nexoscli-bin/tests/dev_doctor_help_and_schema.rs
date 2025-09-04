mod helpers;
use helpers::*;
use std::fs;

#[test]
fn dev_doctor_help_lists_events_checks() {
    let t = tmp();
    let (_code, out, _err) = run_raw(t.path(), &["dev", "doctor", "--help"]);
    assert!(
        out.contains("events-schema"),
        "doctor --help should mention 'events-schema' in --checks help, got:\n{}",
        out
    );
    assert!(
        out.contains("events-chain"),
        "doctor --help should mention 'events-chain' in --checks help, got:\n{}",
        out
    );
}

#[test]
fn events_schema_fails_on_missing_required_fields() {
    let t = tmp();

    // Initialize project structure
    let _ = run_json(t.path(), &["dev", "init", "--yes"]);

    // Create a minimal, invalid event line (missing required fields like exit_code, content_hash)
    let log_dir = t.path().join(".nexoscli").join("log");
    fs::create_dir_all(&log_dir).unwrap();
    let events_path = log_dir.join("events.jsonl");
    let bad_line = r#"{"schema":"event.v0.3","plugin":"test","command":"bad"}"#;
    fs::write(&events_path, format!("{}\n", bad_line)).unwrap();

    // CI verify should fail when requiring events-schema
    let v = run_status_json(
        t.path(),
        &[
            "dev",
            "doctor",
            "--ci",
            "--checks",
            "events-schema",
            "--require",
            "events-schema",
        ],
    );
    if v.status == 0 {
        eprintln!(
            "Expected failure for events-schema with missing fields; got OK. Output:\n{}",
            serde_json::to_string_pretty(&v.json).unwrap_or_else(|_| "<unparseable>".into())
        );
    }
    assert_ne!(v.status, 0, "events-schema should fail on missing required fields");
}
