use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

mod helpers;
use helpers::*;

#[test]
fn test_doctor_redaction_config_present() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create a valid redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns:\n  - \"password\"\n  - \"secret\"").unwrap();
    
    // Run doctor check
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor"]);
    cmd.assert().success();
}

#[test]
fn test_doctor_redaction_config_missing() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Don't create redaction config - should fail
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor"]);
    cmd.assert().failure();
}

#[test]
fn test_doctor_redaction_config_invalid() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create invalid YAML
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "invalid: yaml: content: [").unwrap();
    
    // Should fail due to invalid YAML
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor"]);
    cmd.assert().failure();
}

#[test]
fn test_doctor_events_window() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Generate some events by running commands
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "plugins", "check"]);
    cmd.assert().success();
    
    // Run doctor with time window
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor", "--since", "24h"]);
    cmd.assert().success();
}

#[test]
fn test_doctor_task_coverage() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Start a task
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "start", "--task", "coverage-test"]);
    cmd.assert().success();
    
    // Generate some events with task context
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "plugins", "check"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "status"]);
    cmd.assert().success();
    
    // End the task
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "end"]);
    cmd.assert().success();
    
    // Generate some events without task context
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "plugins", "check"]);
    cmd.assert().success();
    
    // Run doctor with task coverage check
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor", "--task-coverage-min", "30"]);
    cmd.assert().success();
}

#[test]
fn test_doctor_task_coverage_insufficient() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Generate events without task context (no task started)
    for _ in 0..5 {
        let mut cmd = Command::cargo_bin("nexoscli").unwrap();
        cmd.current_dir(&temp_path);
        cmd.args(["dev", "plugins", "check"]);
        cmd.assert().success();
    }
    
    // Should fail with high coverage requirement
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor", "--task-coverage-min", "80"]);
    cmd.assert().failure();
}

#[test]
fn test_doctor_require_flag() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Don't create redaction config
    
    // Run doctor with require flag for redaction only - should fail
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor", "--require", "redaction"]);
    cmd.assert().failure();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Now should succeed with redaction check
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor", "--require", "redaction"]);
    cmd.assert().success();
}

#[test]
fn test_doctor_multiple_checks() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns:\n  - \"api_key\"\n  - \"token\"").unwrap();
    
    // Start task and generate events
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "start", "--task", "multi-test"]);
    cmd.assert().success();
    
    // Generate events with task context
    for _ in 0..3 {
        let mut cmd = Command::cargo_bin("nexoscli").unwrap();
        cmd.current_dir(&temp_path);
        cmd.args(["dev", "plugins", "check"]);
        cmd.assert().success();
    }
    
    // End task
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "end"]);
    cmd.assert().success();
    
    // Run comprehensive doctor check
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args([
        "dev", "doctor",
        "--since", "24h",
        "--task-coverage-min", "50",
        "--require", "redaction,events,coverage"
    ]);
    cmd.assert().success();
}

#[test]
fn test_doctor_output_formats() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Test text output (default)
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor", "--output", "text"]);
    let output = cmd.assert().success();
    
    // Test JSON output
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor", "--output", "json"]);
    let output = cmd.assert().success();
    
    // Could validate JSON format here if needed
}

#[test]
fn test_doctor_time_window_parsing() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Test various time formats
    let time_formats = ["1h", "24h", "7d", "30d"];
    
    for format in &time_formats {
        let mut cmd = Command::cargo_bin("nexoscli").unwrap();
        cmd.current_dir(&temp_path);
        cmd.args(["dev", "doctor", "--since", format]);
        cmd.assert().success();
    }
}

#[test]
fn test_doctor_no_events_file() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Remove events file if it exists
    let events_path = temp_path.join(".nexoscli").join("log").join("events.jsonl");
    if events_path.exists() {
        fs::remove_file(&events_path).unwrap();
    }
    
    // Doctor should still work (with 0 events)
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor"]);
    cmd.assert().success();
}

#[test]
fn test_doctor_malformed_events() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Create redaction config
    let redact_path = temp_path.join(".nexoscli").join("redact.yaml");
    fs::write(&redact_path, "patterns: []").unwrap();
    
    // Create events file with some malformed JSON
    let events_path = temp_path.join(".nexoscli").join("log").join("events.jsonl");
    fs::create_dir_all(events_path.parent().unwrap()).unwrap();
    fs::write(&events_path, 
        "{\"valid\": \"json\"}\n\
         invalid json line\n\
         {\"another\": \"valid\", \"line\": true}\n"
    ).unwrap();
    
    // Doctor should handle malformed events gracefully
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "doctor"]);
    cmd.assert().success();
}
