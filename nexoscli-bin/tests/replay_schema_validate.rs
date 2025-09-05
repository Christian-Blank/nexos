mod helpers;
use helpers::*;
use std::fs;

#[test]
fn replay_schema_validate_events() {
    let t = tmp();

    // init workspace in temp dir
    let _ = run_json(t.path(), &["dev","init","--yes"]);

    // produce a trivial event/run
    let _ = run_json(t.path(), &["term","run","--","echo","hello"]);
    // capture last run_id from events log
    let events = read_events(t.path());
    assert!(!events.is_empty(), "expected at least one event");
    let run_id = events
        .last()
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()))
        .expect("run_id");

    // extract a bag dir
    let outdir = t.path().join("bag");
    let _ = run_json(t.path(), &["replay","extract","--run-id", run_id, "--out", outdir.to_str().unwrap() ]);

    // schema list
    let lst = run_json(t.path(), &["replay","schema","list"]);
    assert!(lst["data"]["schemas"].as_array().unwrap().iter()
        .any(|s| s["id"].as_str()==Some("nexos/events@0.3")),
        "schema list should include nexos/events@0.3, got: {}", lst);

    // schema get (events 0.3)
    let schema_path = t.path().join("events.schema.json");
    let _ = run_json(t.path(), &["replay","schema","get","--name","events","--version","0.3","--out", schema_path.to_str().unwrap()]);
    assert!(schema_path.exists(), "expected schema file to be written");

    // validate (events present; fileops may be absent pre-#35)
    let val = run_json(t.path(), &["replay","validate","--bag", outdir.to_str().unwrap()]);
    assert!(val["data"]["summary_ok"].as_bool().unwrap_or(false), "summary_ok should be true: {}", val);
    assert!(val["data"]["events"]["ok"].as_bool().unwrap(), "events.ok should be true: {}", val);
    // fileops may be null/absent; do not assert on it

    // optional: ensure events.ndjson exists in bag
    assert!(outdir.join("events.ndjson").exists(), "bag should contain events.ndjson");
}
