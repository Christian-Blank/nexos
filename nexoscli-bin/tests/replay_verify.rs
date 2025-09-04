mod helpers;
use helpers::*;
use std::{fs, path::Path};

#[test]
fn replay_verify_ok() {
    let t = tmp();

    // Produce a small run and a TracePack directory
    let v = run_json(t.path(), &["term","run","--fs-topic","--","sh","-c","echo hi > ok.txt"]);
    let run_id = v["meta"]["run_id"].as_str().unwrap();
    let bag_dir = t.path().join("bag");
    let _ = run_json(t.path(), &["replay","extract","--run-id", run_id, "--out", bag_dir.to_str().unwrap()]);

    // Verify via new top-level command
    let rep = run_status_json(t.path(), &["replay","verify","--path", bag_dir.to_str().unwrap()]);
    assert_eq!(rep.status, 0, "expected verify ok: {:?}", rep.json);
    assert!(rep.json["data"]["ok"].as_bool().unwrap(), "expected ok=true: {:?}", rep.json);
}

#[test]
fn replay_verify_fails_when_events_missing() {
    let t = tmp();

    // Produce a run and extract a TracePack
    let v = run_json(t.path(), &["term","run","--","sh","-c","echo hello"]);
    let run_id = v["meta"]["run_id"].as_str().unwrap();
    let bag_dir = t.path().join("bag");
    let _ = run_json(t.path(), &["replay","extract","--run-id", run_id, "--out", bag_dir.to_str().unwrap()]);

    // Remove events file to simulate corruption for TracePack layout (root events.ndjson)
    let ev_path = bag_dir.join("events.ndjson");
    if ev_path.exists() {
        fs::remove_file(&ev_path).unwrap();
    } else {
        // Fallback for bag layout if present (not expected for extract output here)
        let ev_topic = bag_dir.join("topics").join("events.ndjson");
        if ev_topic.exists() {
            fs::remove_file(&ev_topic).unwrap();
        }
    }

    // Verify should fail with error envelope code and exit code 2
    let rep = run_status_json(t.path(), &["replay","verify","--path", bag_dir.to_str().unwrap()]);
    assert_ne!(rep.status, 0, "expected non-zero status on failure: {:?}", rep.json);
    // Envelope error code should be stable for CI gating
    assert_eq!(
        rep.json["errors"][0]["code"].as_str().unwrap_or(""),
        "bag.verify.failed",
        "expected error code bag.verify.failed, got: {:?}",
        rep.json
    );
}
