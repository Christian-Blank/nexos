use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

mod helpers;
use helpers::*;

#[test]
fn test_session_id_correlation_end_to_end() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    let output = cmd.assert().success();
    
    // Start a task (should generate session_id)
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "start", "--name", "session-test"]);
    let output = cmd.assert().success();
    
    // Verify current_session file exists
    let session_path = temp_path.join(".nexoscli").join("task").join("current_session");
    assert!(session_path.exists(), "Current session file should exist after task start");
    
    // Read the session ID
    let session_id = fs::read_to_string(&session_path).unwrap().trim().to_string();
    assert!(session_id.starts_with("SES-"), "Session ID should start with SES-");
    assert_eq!(session_id.len(), 16, "Session ID should be 16 characters: SES- + 12 hex chars");
    
    // Run some commands that should get context injection
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "plugins", "check"]);
    cmd.assert().success();
    
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "status"]);
    cmd.assert().success();
    
    // Check events file for context injection
    let events_path = temp_path.join(".nexoscli").join("log").join("events.jsonl");
    assert!(events_path.exists(), "Events file should exist");
    
    let events_content = fs::read_to_string(&events_path).unwrap();
    let events: Vec<&str> = events_content.lines().collect();
    
    // Find events with task context (should include session_id)
    let mut session_events = 0;
    for line in events {
        if line.contains(&session_id) && line.contains("\"context\"") {
            session_events += 1;
        }
    }
    
    assert!(session_events > 0, "Should have events with session_id in context");
    
    // Pause task (should clear current_session)
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "pause"]);
    cmd.assert().success();
    
    // Verify current_session file is removed
    assert!(!session_path.exists(), "Current session file should be removed after task pause");
    
    // Resume task (should generate new session_id)
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "resume", "--name", "session-test"]);
    cmd.assert().success();
    
    // Verify new session file exists
    assert!(session_path.exists(), "Current session file should exist after task resume");
    
    // Read new session ID
    let new_session_id = fs::read_to_string(&session_path).unwrap().trim().to_string();
    assert!(new_session_id.starts_with("SES-"), "New session ID should start with SES-");
    assert_ne!(session_id, new_session_id, "New session ID should be different from original");
    
    // End task (should clear current_session)
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "end"]);
    cmd.assert().success();
    
    // Verify current_session file is removed
    assert!(!session_path.exists(), "Current session file should be removed after task end");
    
    // Check task bundle includes session_id
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["task", "bundle", "--name", "session-test"]);
    let output = cmd.assert().success();
    
    // Find and extract the bundle
    let bundle_files: Vec<_> = fs::read_dir(&temp_path)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? == "tgz" && path.file_name()?.to_str()?.contains("session-test") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    
    assert!(!bundle_files.is_empty(), "Should have generated a task bundle");
    
    // Extract and check manifest for session_id
    let bundle_path = &bundle_files[0];
    let extract_dir = temp_path.join("extract");
    fs::create_dir_all(&extract_dir).unwrap();
    
    let tar_gz = fs::File::open(bundle_path).unwrap();
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);
    archive.unpack(&extract_dir).unwrap();
    
    let manifest_path = extract_dir.join("manifest.json");
    assert!(manifest_path.exists(), "Bundle should contain manifest.json");
    
    let manifest_content = fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_content.contains("session_id"), "Manifest should contain session_id field");
    
    // Verify session_id is in the manifest (could be either original or new one)
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content).unwrap();
    let manifest_session_id = manifest.get("session_id").and_then(|v| v.as_str());
    assert!(manifest_session_id.is_some(), "Manifest should have session_id field");
    
    let session_id_val = manifest_session_id.unwrap();
    assert!(session_id_val.starts_with("SES-"), "Manifest session_id should start with SES-");
    assert!(session_id_val == session_id || session_id_val == new_session_id, 
           "Manifest session_id should match one of the generated session IDs");
}

#[test] 
fn test_session_id_no_active_task() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    // Run command without active task
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "plugins", "check"]);
    cmd.assert().success();
    
    // Verify no current_session file
    let session_path = temp_path.join(".nexoscli").join("task").join("current_session");
    assert!(!session_path.exists(), "Current session file should not exist without active task");
    
    // Check events don't have session context
    let events_path = temp_path.join(".nexoscli").join("log").join("events.jsonl");
    if events_path.exists() {
        let events_content = fs::read_to_string(&events_path).unwrap();
        // Events should not contain session_id when no task is active
        for line in events_content.lines() {
            if line.contains("\"context\"") {
                let event: serde_json::Value = serde_json::from_str(line).unwrap();
                if let Some(context) = event.get("context") {
                    assert!(context.get("session_id").is_none(), 
                           "Events should not have session_id context when no task is active");
                }
            }
        }
    }
}

#[test]
fn test_session_id_generation_uniqueness() {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();
    
    // Initialize nexos
    let mut cmd = Command::cargo_bin("nexoscli").unwrap();
    cmd.current_dir(&temp_path);
    cmd.args(["dev", "init", "--yes"]);
    cmd.assert().success();
    
    let session_path = temp_path.join(".nexoscli").join("task").join("current_session");
    let mut session_ids = Vec::new();
    
    // Generate multiple sessions
    for i in 0..5 {
        let mut cmd = Command::cargo_bin("nexoscli").unwrap();
        cmd.current_dir(&temp_path);
        cmd.args(["task", "start", "--name", &format!("unique-test-{}", i)]);
        cmd.assert().success();
        
        assert!(session_path.exists(), "Session file should exist");
        let session_id = fs::read_to_string(&session_path).unwrap().trim().to_string();
        session_ids.push(session_id.clone());
        
        let mut cmd = Command::cargo_bin("nexoscli").unwrap();
        cmd.current_dir(&temp_path);
        cmd.args(["task", "end"]);
        cmd.assert().success();
        
        // Small delay to ensure different timestamps
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    
    // Verify all session IDs are unique
    for (i, id1) in session_ids.iter().enumerate() {
        for (j, id2) in session_ids.iter().enumerate() {
            if i != j {
                assert_ne!(id1, id2, "Session IDs should be unique: {} vs {}", id1, id2);
            }
        }
    }
}
