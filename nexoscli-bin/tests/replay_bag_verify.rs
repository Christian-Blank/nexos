mod helpers;
use helpers::*;
use std::fs;
use std::path::Path;

fn write_file(p: &Path, s: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, s.as_bytes()).unwrap();
}

#[test]
fn replay_bag_verify_ok_then_fail_on_tamper() {
    let t = tmp();
    let bag = t.path().join("tracepack");
    let topics = bag.join("topics");
    fs::create_dir_all(&topics).unwrap();

    // Minimal v0.2-style bag manifest + events topic
    let manifest = r#"
{
  "schema": "bag_manifest.v0.2",
  "version": "0.2",
  "created_at": "1970-01-01T00:00:00Z",
  "format": "dir",
  "counts": { "events": 2 },
  "time_range": { "start": "1970-01-01T00:00:00Z", "end": "1970-01-01T00:00:01Z" },
  "topics": { "events": { "path": "topics/events.ndjson", "schema": "event_v0.3" } }
}
"#.trim_start();
    write_file(&bag.join("manifest.json"), manifest);

    // Two events with ts only (chain fields omitted; chain_ok may be false but does not fail ok)
    let events = r#"
{"ts":"1970-01-01T00:00:00Z","plugin":"demo","command":"a","exit_code":0}
{"ts":"1970-01-01T00:00:01Z","plugin":"demo","command":"b","exit_code":0}
"#.trim_start();
    write_file(&topics.join("events.ndjson"), events);

    // Verify should be OK
    let ok = run_status_json(t.path(), &["replay","bag","verify","--bag", bag.to_str().unwrap()]);
    assert_eq!(ok.status, 0, "expected verify ok: {:?}", ok.json);
    assert_eq!(ok.json["data"]["ok"].as_bool().unwrap_or(false), true);

    // Tamper: append one extra line to events (counts mismatch)
    let more = r#"
{"ts":"1970-01-01T00:00:01Z","plugin":"demo","command":"c","exit_code":0}
"#.trim_start();
    let ev_path = topics.join("events.ndjson");
    let mut cur = fs::read_to_string(&ev_path).unwrap();
    cur.push_str(more);
    fs::write(&ev_path, cur.as_bytes()).unwrap();

    // Now should fail (non-zero status, code 2)
    let bad = run_status_json(t.path(), &["replay","bag","verify","--bag", bag.to_str().unwrap()]);
    assert_ne!(bad.status, 0, "expected verify fail: {:?}", bad.json);
}
