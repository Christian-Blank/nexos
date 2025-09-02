mod helpers; use helpers::*;
use std::{fs, process::Command};

#[test]
fn temporal_engine_writes_outbox() {
    let t = tmp();
    // basic repo
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    // create task and start with temporal engine
    let j = run_json(t.path(), &["task","new","--title","Outbox"]);
    let tid = j["data"]["id"].as_str().unwrap();
    run_ok(t.path(), &["task","start","--task",tid,"--engine","temporal"]);
    // stage with temporal
    run_ok(t.path(), &["task","stage","implementation","--task",tid,"--engine","temporal"]);

    // assert outbox has at least 2 envelopes
    let outbox = t.path().join(".nexoscli/engine/outbox");
    let entries: Vec<_> = fs::read_dir(&outbox).unwrap().collect();
    assert!(entries.len() >= 2, "expected outbox envelopes, found {}", entries.len());
    // sanity: check structure
    let first = fs::read_to_string(entries[0].as_ref().unwrap().path()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert!(v.get("op").is_some());
    assert_eq!(v.get("workflow").unwrap().as_str().unwrap(), "task");
}

#[test]
fn local_engine_runs_hooks_unchanged() {
    let t = tmp();
    // For now just test that local engine doesn't write to outbox
    // TODO: Add hooks test when hook system is implemented
    
    let j = run_json(t.path(), &["task","new","--title","LocalHooks"]);
    let tid = j["data"]["id"].as_str().unwrap();
    run_ok(t.path(), &["task","start","--task",tid,"--engine","local"]);
    run_ok(t.path(), &["task","stage","planning","--task",tid,"--engine","local"]);
    
    // assert no outbox directory was created
    let outbox = t.path().join(".nexoscli/engine/outbox");
    assert!(!outbox.exists(), "local engine should not create outbox directory");
}

#[test]
fn temporal_engine_end_writes_signal() {
    let t = tmp();
    // basic repo
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    // create task and start with temporal engine
    let j = run_json(t.path(), &["task","new","--title","EndTest"]);
    let tid = j["data"]["id"].as_str().unwrap();
    run_ok(t.path(), &["task","start","--task",tid,"--engine","temporal"]);
    
    // end with temporal
    run_ok(t.path(), &["task","end","--task",tid,"--engine","temporal"]);

    // assert outbox has at least 2 envelopes (start + end)
    let outbox = t.path().join(".nexoscli/engine/outbox");
    let entries: Vec<_> = fs::read_dir(&outbox).unwrap().collect();
    assert!(entries.len() >= 2, "expected start + end envelopes, found {}", entries.len());
    
    // check that we have both start and signal operations
    let mut has_start = false;
    let mut has_signal = false;
    
    for entry in entries {
        let path = entry.unwrap().path();
        let content = fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        let op = v.get("op").unwrap().as_str().unwrap();
        match op {
            "start" => has_start = true,
            "signal" => has_signal = true,
            _ => {}
        }
    }
    
    assert!(has_start, "expected start envelope");
    assert!(has_signal, "expected signal envelope for end");
}

#[test]
fn outbox_envelope_structure() {
    let t = tmp();
    // basic repo
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "x\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    let j = run_json(t.path(), &["task","new","--title","StructureTest"]);
    let tid = j["data"]["id"].as_str().unwrap();
    run_ok(t.path(), &["task","start","--task",tid,"--engine","temporal"]);

    // check envelope structure
    let outbox = t.path().join(".nexoscli/engine/outbox");
    let entries: Vec<_> = fs::read_dir(&outbox).unwrap().collect();
    let first_path = entries[0].as_ref().unwrap().path();
    let content = fs::read_to_string(&first_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    
    // Required fields
    assert!(v.get("op").is_some());
    assert!(v.get("workflow").is_some());
    assert!(v.get("key").is_some());
    assert!(v.get("attrs").is_some());
    assert!(v.get("body").is_some());
    assert!(v.get("ts").is_some());
    
    // Check specific values
    assert_eq!(v.get("op").unwrap().as_str().unwrap(), "start");
    assert_eq!(v.get("workflow").unwrap().as_str().unwrap(), "task");
    assert_eq!(v.get("key").unwrap().as_str().unwrap(), tid);
    
    // Check attrs structure
    let attrs = v.get("attrs").unwrap();
    assert!(attrs.get("task_id").is_some());
    assert!(attrs.get("stage").is_some());
    assert!(attrs.get("status").is_some());
    assert!(attrs.get("session_id").is_some());
}
