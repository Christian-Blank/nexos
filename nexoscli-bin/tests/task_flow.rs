mod helpers; 
use helpers::*;
use std::fs;
use std::process::Command;
use serde_json::Value;

#[test]
fn task_lifecycle_and_context() {
    let t = tmp();

    // Step 1: Create a new task
    let j = run_json(t.path(), &["task","new","--title","Scaffold plugin"]);
    let task = j["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(j["errors"], Value::Array(vec![]));
    assert_eq!(j["data"]["stage"].as_str().unwrap(), "planning");
    assert_eq!(j["data"]["status"].as_str().unwrap(), "new");

    // Step 2: Start the task
    let j2 = run_json(t.path(), &["task","start","--task",&task]);
    assert_eq!(j2["errors"], Value::Array(vec![]));
    assert_eq!(j2["data"]["status"].as_str().unwrap(), "active");
    assert_eq!(j2["data"]["id"].as_str().unwrap(), task);

    // Step 3: Change stage to implementation
    let j3 = run_json(t.path(), &["task","stage","implementation","--task",&task]);
    assert_eq!(j3["errors"], Value::Array(vec![]));
    assert_eq!(j3["data"]["stage"].as_str().unwrap(), "implementation");

    // Step 4: Initialize git and make a change via term run (should carry task context)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "one\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    let v = run_json(t.path(), &["term","run","sh","-lc","echo two >> a.txt"]);
    assert_eq!(v["errors"], Value::Array(vec![]));
    
    // CRUCIAL: Verify context injection in JSON envelope
    assert_eq!(v["meta"]["context"]["task_id"].as_str().unwrap(), task);
    assert_eq!(v["meta"]["context"]["stage"].as_str().unwrap(), "implementation");

    // Step 5: Verify context injection in events
    let events = read_events(t.path());
    let term_events: Vec<&Value> = events.iter().filter(|e| 
        e.get("plugin").and_then(|x| x.as_str()) == Some("term") &&
        e.get("command").and_then(|x| x.as_str()) == Some("run")
    ).collect();
    assert!(!term_events.is_empty(), "should have term run events");
    
    // Find the term run event and verify it has context
    let term_event = term_events.last().unwrap();
    assert_eq!(term_event["context"]["task_id"].as_str().unwrap(), task);
    assert_eq!(term_event["context"]["stage"].as_str().unwrap(), "implementation");

    // Step 6: Pause the task
    let j4 = run_json(t.path(), &["task","pause","--task",&task]);
    assert_eq!(j4["errors"], Value::Array(vec![]));
    assert_eq!(j4["data"]["status"].as_str().unwrap(), "paused");

    // Step 7: Resume the task
    let j5 = run_json(t.path(), &["task","resume","--task",&task]);
    assert_eq!(j5["errors"], Value::Array(vec![]));
    assert_eq!(j5["data"]["status"].as_str().unwrap(), "active");

    // Step 8: Run another command with task active to test context again
    let v2 = run_json(t.path(), &["term","run","sh","-lc","echo three >> a.txt"]);
    assert_eq!(v2["errors"], Value::Array(vec![]));
    assert_eq!(v2["meta"]["context"]["task_id"].as_str().unwrap(), task);
    assert_eq!(v2["meta"]["context"]["stage"].as_str().unwrap(), "implementation");

    // Step 9: End the task
    let j6 = run_json(t.path(), &["task","end","--task",&task]);
    assert_eq!(j6["errors"], Value::Array(vec![]));
    assert_eq!(j6["data"]["status"].as_str().unwrap(), "ended");
    assert_eq!(j6["data"]["stage"].as_str().unwrap(), "done");

    // Step 10: Verify task status shows sessions and counts
    let j7 = run_json(t.path(), &["task","status","--task",&task]);
    assert_eq!(j7["errors"], Value::Array(vec![]));
    assert_eq!(j7["data"]["id"].as_str().unwrap(), task);
    assert!(j7["data"]["sessions"].as_array().unwrap().len() >= 2); // start + resume sessions
    assert!(j7["data"]["counts"]["events"].as_u64().unwrap() > 0);

    // Step 11: Test bundle creation (directory output)
    let outdir = t.path().join("packs").join("task1");
    let b = run_json(t.path(), &["task","bundle","--task",&task,"--out", outdir.to_str().unwrap()]);
    assert_eq!(b["errors"], Value::Array(vec![]));
    assert_eq!(b["data"]["task"].as_str().unwrap(), task);

    // Verify bundle structure
    assert!(outdir.join("manifest.json").exists());
    assert!(outdir.join("events.ndjson").exists());
    assert!(outdir.join("task").join("task.json").exists());
    assert!(outdir.join("README.md").exists());

    // Verify manifest content
    let manifest: Value = serde_json::from_slice(&fs::read(outdir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["kind"].as_str().unwrap(), "tracepack.task");
    assert_eq!(manifest["task_id"].as_str().unwrap(), task);
    assert!(manifest["counts"]["events_lines"].as_u64().unwrap() > 0);

    export_events(t.path(), "task_lifecycle");
}

#[test]
fn task_lifecycle_without_active_task() {
    let t = tmp();

    // Commands without active task should not have context
    let v = run_json(t.path(), &["issue","new","--title","test"]);
    assert_eq!(v["errors"], Value::Array(vec![]));
    assert!(v["meta"]["context"].is_null());

    // Verify in events too
    let events = read_events(t.path());
    let issue_events: Vec<&Value> = events.iter().filter(|e| 
        e.get("plugin").and_then(|x| x.as_str()) == Some("issue") &&
        e.get("command").and_then(|x| x.as_str()) == Some("new")
    ).collect();
    assert!(!issue_events.is_empty());
    let issue_event = issue_events.last().unwrap();
    assert!(issue_event.get("context").is_none());
}

#[test]
fn task_concurrent_active_prevention() {
    let t = tmp();

    // Create and start first task
    let j1 = run_json(t.path(), &["task","new","--title","First task"]);
    let task1 = j1["data"]["id"].as_str().unwrap().to_string();
    run_json(t.path(), &["task","start","--task",&task1]);

    // Create second task
    let j2 = run_json(t.path(), &["task","new","--title","Second task"]);
    let task2 = j2["data"]["id"].as_str().unwrap().to_string();

    // Try to start second task while first is active - should fail
    let (code, _stdout, stderr) = run_raw(t.path(), &["task","start","--task",&task2]);
    assert_ne!(code, 0);
    assert!(stderr.contains("is active; pause it"));

    // Pause first task, then start second - should work
    run_json(t.path(), &["task","pause","--task",&task1]);
    let j3 = run_json(t.path(), &["task","start","--task",&task2]);
    assert_eq!(j3["errors"], Value::Array(vec![]));
    assert_eq!(j3["data"]["status"].as_str().unwrap(), "active");
}

#[test]
fn task_list_and_status() {
    let t = tmp();

    // Create multiple tasks
    let j1 = run_json(t.path(), &["task","new","--title","First task"]);
    let task1 = j1["data"]["id"].as_str().unwrap().to_string();
    
    let j2 = run_json(t.path(), &["task","new","--title","Second task"]);
    let task2 = j2["data"]["id"].as_str().unwrap().to_string();

    // List should show both
    let list = run_json(t.path(), &["task","list"]);
    assert_eq!(list["errors"], Value::Array(vec![]));
    let items = list["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    // Status should work for both
    let s1 = run_json(t.path(), &["task","status","--task",&task1]);
    assert_eq!(s1["errors"], Value::Array(vec![]));
    assert_eq!(s1["data"]["id"].as_str().unwrap(), task1);
    assert_eq!(s1["data"]["status"].as_str().unwrap(), "new");

    let s2 = run_json(t.path(), &["task","status","--task",&task2]);
    assert_eq!(s2["errors"], Value::Array(vec![]));
    assert_eq!(s2["data"]["id"].as_str().unwrap(), task2);
    assert_eq!(s2["data"]["status"].as_str().unwrap(), "new");
}

#[test]
fn task_bundle_tgz_output() {
    let t = tmp();

    // Create and run a minimal task workflow
    let j = run_json(t.path(), &["task","new","--title","Bundle test"]);
    let task = j["data"]["id"].as_str().unwrap().to_string();
    
    run_json(t.path(), &["task","start","--task",&task]);
    run_json(t.path(), &["task","end","--task",&task]);

    // Bundle as .tgz
    let tgz_path = t.path().join("bundle.tgz");
    let b = run_json(t.path(), &["task","bundle","--task",&task,"--out", tgz_path.to_str().unwrap()]);
    assert_eq!(b["errors"], Value::Array(vec![]));
    assert!(tgz_path.exists());
    assert!(tgz_path.metadata().unwrap().len() > 0);
}
