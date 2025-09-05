mod helpers;
use helpers::*;
use std::fs;
use std::process::Command;

#[test]
fn replay_export_csv_events_and_fileops() {
    let t = tmp();

    // Init a git repo and make an initial commit so patches can be generated
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "one\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    // Run a command that changes files and emits fs-topic so fileops will exist in extract
    // This ensures bag/fileops/fileops.ndjson is present after extraction.
    let j = run_json(
        t.path(),
        &[
            "term","run","--fs-topic","sh","-lc",
            "echo two >> a.txt && mv a.txt b.txt && rm -f b.txt"
        ]
    );
    let run_id = j["meta"]["run_id"].as_str().expect("run_id");

    // Extract a TracePack directory
    let outdir = t.path().join("packs").join("run");
    let _ = run_json(
        t.path(),
        &["replay","extract","--run-id", run_id, "--out", outdir.to_str().unwrap()]
    );

    // Sanity: events and fileops should exist (per PLAN-35 + fs-topic)
    assert!(outdir.join("events.ndjson").exists(), "expected events.ndjson in TracePack");
    assert!(outdir.join("fileops").join("fileops.ndjson").exists(), "expected fileops/fileops.ndjson in TracePack");

    // Export CSV for both artifacts
    let v = run_json(
        t.path(),
        &["replay","export","--bag", outdir.to_str().unwrap(), "--what", "all", "--as", "csv"]
    );

    // Envelope checks
    assert_eq!(v["data"]["format"].as_str().unwrap_or(""), "csv");
    let written = v["data"]["written"].as_array().cloned().unwrap_or_default();
    assert!(written.len() >= 2, "expected two written paths (events.csv, fileops.csv), got: {:?}", written);
    let events_rows = v["data"]["counts"]["events"].as_u64().unwrap_or(0);
    let fileops_rows = v["data"]["counts"]["fileops"].as_u64().unwrap_or(0);
    // we expect at least one event and one fileop row
    assert!(events_rows >= 1, "expected >=1 events rows, got {}", events_rows);
    assert!(fileops_rows >= 1, "expected >=1 fileops rows, got {}", fileops_rows);

    // File existence
    let exports = outdir.join("exports").join("csv");
    let events_csv = exports.join("events.csv");
    let fileops_csv = exports.join("fileops.csv");
    assert!(events_csv.exists(), "missing {}", events_csv.display());
    assert!(fileops_csv.exists(), "missing {}", fileops_csv.display());

    // Header validation and at least one data row
    let events_body = fs::read_to_string(&events_csv).expect("read events.csv");
    let mut ev_lines = events_body.lines();
    let ev_header = ev_lines.next().unwrap_or("");
    assert_eq!(
        ev_header,
        "ts,run_id,plugin,command,exit_code,dur_ms,error_code,args_hash,content_hash,task_id,stage,session_id,attrs_cmd,attrs_cwd,attrs_json",
        "events.csv header mismatch: {}",
        ev_header
    );
    assert!(ev_lines.next().is_some(), "events.csv should contain at least one data row");

    let fileops_body = fs::read_to_string(&fileops_csv).expect("read fileops.csv");
    let mut fo_lines = fileops_body.lines();
    let fo_header = fo_lines.next().unwrap_or("");
    assert_eq!(
        fo_header,
        "ts,run_id,op,path,from,to,bytes,digest_after,patch_sha256,patch_lines,source",
        "fileops.csv header mismatch: {}",
        fo_header
    );
    assert!(fo_lines.next().is_some(), "fileops.csv should contain at least one data row");
}
