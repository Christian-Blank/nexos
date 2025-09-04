mod helpers;
use helpers::*;
use std::fs;

#[test]
fn dev_log_rotate_and_verify() {
    let t = tmp();

    // init + create some events
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    let _ = run_json(t.path(), &["task","new","--title","rotate smoke"]);
    let _ = run_json(t.path(), &["task","start","--task","TASK-19700101-rotate-smoke"]);

    // force tiny threshold to rotate immediately
    let v1 = run_status_json(t.path(), &["dev","log","rotate","--max-bytes","1"]);
    if v1.status != 0 {
        eprintln!("rotate failed: {}", serde_json::to_string_pretty(&v1.json).unwrap_or_else(|_| "<invalid json>".into()));
    }
    assert_eq!(v1.status, 0, "rotation should succeed");
    assert!(t.path().join(".nexoscli/log/events.index.json").exists(), "events.index.json should be created");

    // CI verify rotation continuity
    let ci = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-rotation","--require","events-rotation"]);
    if ci.status != 0 {
        eprintln!("doctor verify output: {}", serde_json::to_string_pretty(&ci.json).unwrap_or_else(|_| "<invalid json>".into()));
    }
    assert_eq!(ci.status, 0, "events-rotation verify should pass");

    // tamper: change first segment file → verify must fail
    let idx_path = t.path().join(".nexoscli/log/events.index.json");
    let idx: serde_json::Value = serde_json::from_str(&fs::read_to_string(idx_path).unwrap()).unwrap();
    let seg_file = idx["segments"][0]["file"].as_str().expect("segment file path");
    // Overwrite with a single valid-looking line to change sha/lines
    fs::write(t.path().join(seg_file), b"{\"ts\":\"1970-01-01T00:00:00Z\",\"schema\":\"event.v0.3\",\"plugin\":\"tamper\",\"command\":\"x\",\"exit_code\":0}\n").unwrap();

    let ci2 = run_status_json(t.path(), &["dev","doctor","--ci","--checks","events-rotation","--require","events-rotation"]);
    assert_ne!(ci2.status, 0, "events-rotation verify must fail after tamper");
}
