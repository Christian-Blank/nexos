mod helpers;
use helpers::*;
use std::fs;
use std::io::Write as IoWrite;

#[test]
fn doctor_events_schema_and_chain() {
    let t = tmp();

    // Initialize repo
    let _ = run_json(t.path(), &["dev", "init", "--yes"]);

    // Redaction config required by non-CI doctor (hard requirement by default)
    fs::create_dir_all(t.path().join(".nexoscli")).unwrap();
    fs::write(t.path().join(".nexoscli").join("redact.yaml"), "patterns: []").unwrap();

    // Generate some events
    let _ = run_json(t.path(), &["task", "new", "--title", "doctor chain smoke"]);
    let _ = run_json(t.path(), &["task", "start", "--task", "TASK-19700101-doctor-chain-smoke"]);

    // 1) Local run creates/refreshes chain file
    let v1 = run_status_json(t.path(), &["dev", "doctor", "--checks", "events-schema,events-chain"]);
    assert_eq!(v1.status, 0, "local doctor should succeed");
    let chain = t.path().join(".nexoscli").join("log").join("events.chain.json");
    assert!(chain.exists(), "local doctor should write chain");

    // 2) CI mode verifies and passes
    let v2 = run_status_json(
        t.path(),
        &[
            "dev",
            "doctor",
            "--ci",
            "--checks",
            "events-schema,events-chain",
            "--require",
            "events-schema,events-chain",
        ],
    );
    if v2.status != 0 {
        eprintln!("CI doctor output: {}", serde_json::to_string_pretty(&v2.json).unwrap_or_else(|_| "<unparseable>".into()));
    }
    assert_eq!(v2.status, 0, "CI verify should pass when chain matches");

    // 3) Tamper with events.jsonl to cause a chain mismatch
    let events_path = t.path().join(".nexoscli").join("log").join("events.jsonl");
    let mut f = fs::OpenOptions::new().append(true).open(&events_path).unwrap();
    // Append a valid-looking event.v0.3 line outside the chain
    let ev = r#"{"ts":"1970-01-01T00:00:00Z","schema":"event.v0.3","plugin":"test","command":"tamper","exit_code":0}"#;
    writeln!(f, "{}", ev).unwrap();

    // CI verify must now fail on chain drift
    let v3 = run_status_json(
        t.path(),
        &["dev", "doctor", "--ci", "--checks", "events-chain", "--require", "events-chain"],
    );
    assert_ne!(v3.status, 0, "CI verify must fail on chain drift");
}
