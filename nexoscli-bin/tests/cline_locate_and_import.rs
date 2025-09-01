mod helpers; use helpers::*;
use std::fs;

#[test]
fn cline_locate_via_test_root_and_idempotent_import() {
    let t = tmp();

    // Simulate a globalStorage root with ext id
    let root = t.path().join("globalStorage").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&root).unwrap();
    // First task
    fs::write(root.join("task-1.json"), br#"{
      "id":"task-1","title":"First","createdAt":"2025-08-30T12:00:00Z","updatedAt":"2025-08-30T12:10:00Z",
      "messages":[{"role":"user","content":"hi"}],"token":"secret"
    }"#).unwrap();

    // Make Cline discovery point here
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", t.path().join("globalStorage").to_str().unwrap());

    // locate should find it
    let v = run_json(t.path(), &["cline","locate"]);
    assert!(v["data"]["selected"].as_str().is_some());

    // import once -> created = 1
    let v1 = run_json(t.path(), &["cline","import","--storage", root.to_str().unwrap()]);
    assert_eq!(v1["data"]["created"].as_u64().unwrap(), 1);
    // secrets redacted in normalized copy
    let norm = t.path().join(".nexoscli/cline/tasks").join("task-1.json");
    let body = std::fs::read_to_string(norm).unwrap();
    assert!(body.contains("***redacted***"));

    // import again without change -> unchanged = 1
    let v2 = run_json(t.path(), &["cline","import","--storage", root.to_str().unwrap()]);
    assert_eq!(v2["data"]["unchanged"].as_u64().unwrap(), 1);

    // modify file -> changed = 1
    fs::write(root.join("task-1.json"), br#"{"id":"task-1","updatedAt":"2025-08-30T12:20:00Z","messages":[]}"#).unwrap();
    let v3 = run_json(t.path(), &["cline","import","--storage", root.to_str().unwrap()]);
    assert_eq!(v3["data"]["changed"].as_u64().unwrap(), 1);
}
