mod helpers;
use helpers::*;
use std::fs;
use std::path::Path;

#[test]
fn cline_import_reads_tasks_dir() {
    let t = tmp();
    // fake Cline storage
    let storage = t.path().join("cline-storage").join("tasks");
    fs::create_dir_all(&storage).unwrap();
    fs::write(storage.join("task-123.json"), r#"{
      "id":"task-123","title":"Demo","createdAt":"2025-08-30T12:34:56Z",
      "updatedAt":"2025-08-30T13:00:00Z","messages":[{"role":"user","content":"Hi"}],
      "creds":{"apiKey":"secret"}
    }"#).unwrap();

    let v = run_json(t.path(), &["cline","import","--storage", storage.to_str().unwrap()]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    assert!(v["data"]["imported"].as_u64().unwrap() >= 1);
    let tasks = v["data"]["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["id"], "task-123");
    // normalized copy present
    let norm = t.path().join(".nexoscli").join("cline").join("tasks").join("task-123.json");
    assert!(Path::new(&norm).exists());
    // redaction
    let body = fs::read_to_string(norm).unwrap();
    assert!(body.contains("***redacted***"));
}

#[test]
fn cline_locate_finds_candidates() {
    let t = tmp();
    let v = run_json(t.path(), &["cline","locate"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    // Should have candidates array (may be empty on test system)
    assert!(v["data"]["candidates"].is_array());
}

#[test]
fn cline_import_dry_run() {
    let t = tmp();
    let storage = t.path().join("cline-storage").join("tasks");
    fs::create_dir_all(&storage).unwrap();
    fs::write(storage.join("task-456.json"), r#"{
      "id":"task-456","title":"Dry run test","createdAt":"2025-08-30T14:00:00Z",
      "messages":[{"role":"user","content":"Test message"}]
    }"#).unwrap();

    let v = run_json(t.path(), &["cline","import","--storage", storage.to_str().unwrap(), "--dry-run"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    assert!(v["data"]["imported"].as_u64().unwrap() >= 1);
    
    // Should not create actual files in dry-run mode
    let norm = t.path().join(".nexoscli").join("cline").join("tasks").join("task-456.json");
    assert!(!Path::new(&norm).exists());
}

#[test]
fn cline_import_with_since_filter() {
    let t = tmp();
    let storage = t.path().join("cline-storage").join("tasks");
    fs::create_dir_all(&storage).unwrap();
    let old_file = storage.join("old-task.json");
    fs::write(&old_file, r#"{
      "id":"old-task","title":"Old task","createdAt":"2020-01-01T00:00:00Z",
      "messages":[]
    }"#).unwrap();
    
    // Set the file's modification time to be old (2020)
    use filetime::{FileTime, set_file_mtime};
    let old_time = FileTime::from_unix_time(1577836800, 0); // 2020-01-01 00:00:00 UTC
    let _ = set_file_mtime(&old_file, old_time);

    // Import only from last 24h - should skip the old task
    let v = run_json(t.path(), &["cline","import","--storage", storage.to_str().unwrap(), "--since", "24h"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    assert_eq!(v["data"]["skipped"].as_u64().unwrap(), 1);
    assert_eq!(v["data"]["imported"].as_u64().unwrap(), 0);
}

#[test]
fn cline_import_with_limit() {
    let t = tmp();
    let storage = t.path().join("cline-storage").join("tasks");
    fs::create_dir_all(&storage).unwrap();
    
    // Create multiple tasks
    for i in 1..=5 {
        fs::write(storage.join(format!("task-{}.json", i)), format!(r#"{{
          "id":"task-{}","title":"Task {}","createdAt":"2025-08-30T12:34:56Z",
          "messages":[]
        }}"#, i, i)).unwrap();
    }

    let v = run_json(t.path(), &["cline","import","--storage", storage.to_str().unwrap(), "--limit", "2"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    assert!(v["data"]["imported"].as_u64().unwrap() >= 2);
    let tasks = v["data"]["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2); // Limited to 2 tasks in response
}
