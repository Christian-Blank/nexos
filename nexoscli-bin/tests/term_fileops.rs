mod helpers;
use helpers::*;
use std::{fs, thread, time::Duration};

#[test]
fn term_run_emits_fileops_when_enabled() {
    let t = tmp();

    // Run a shell script that creates, renames, and removes a file while fs-topic is enabled
    let _ = run_json(
        t.path(),
        &[
            "term",
            "run",
            "--fs-topic",
            "sh",
            "-c",
            "echo hi > a.txt; mv a.txt b.txt; rm b.txt",
        ],
    );

    // Give the watcher a short moment to flush
    thread::sleep(Duration::from_millis(200));

    // Verify .nexoscli/fileops contains a fileops stream with meta and at least one op line
    let dir = t.path().join(".nexoscli").join("fileops");
    assert!(dir.exists(), "fileops dir not found at {}", dir.display());
    let mut found = false;
    for ent in fs::read_dir(&dir).unwrap() {
        let p = ent.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) == Some("ndjson") {
            let body = fs::read_to_string(&p).unwrap_or_default();
            assert!(
                body.contains("\"phase\":\"start\""),
                "missing start meta in {}",
                p.display()
            );
            assert!(
                body.contains("\"op\":\"create\"")
                    || body.contains("\"op\":\"modify\"")
                    || body.contains("\"op\":\"delete\"")
                    || body.contains("\"op\":\"rename\""),
                "no fileops operations captured in {}:\n{}",
                p.display(),
                body
            );
            found = true;
        }
    }
    assert!(found, "no fileops stream found in {}", dir.display());

    // Ensure a term.fs.topic event was emitted for this run
    let evs = read_events(t.path());
    let run_id = evs
        .iter()
        .find(|v| {
            v.get("plugin").and_then(|s| s.as_str()) == Some("term")
                && v.get("command").and_then(|s| s.as_str()) == Some("run")
        })
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(!run_id.is_empty(), "no run event captured to recover run_id");
    assert_event_for_run(t.path(), &run_id, "term", "fs.topic");
}
