mod helpers;
use helpers::*;
use std::fs;

#[test]
fn replay_extract_includes_fileops_when_fs_topic_enabled() {
    let t = tmp();

    // Produce a run with fs-topic enabled that creates and removes a file
    let v = run_json(
        t.path(),
        &[
            "term",
            "run",
            "--fs-topic",
            "sh",
            "-c",
            "echo hi > z.txt; rm z.txt",
        ],
    );
    let run_id = v["meta"]["run_id"].as_str().unwrap_or("");

    // Extract a TracePack directory
    let outdir = t.path().join("packs").join("with-fileops");
    let _ = run_json(
        t.path(),
        &[
            "replay",
            "extract",
            "--run-id",
            run_id,
            "--out",
            outdir.to_str().unwrap(),
        ],
    );

    // Verify fileops was included
    let fo = outdir.join("fileops").join("fileops.ndjson");
    assert!(fo.exists(), "expected fileops/fileops.ndjson to exist at {}", fo.display());

    // Manifest should include counts.fileops_lines and a fileops object
    let manifest_text = fs::read_to_string(outdir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();

    assert!(
        manifest.pointer("/counts/fileops_lines").is_some(),
        "manifest missing counts.fileops_lines: {}",
        manifest_text
    );
    assert!(
        manifest.get("fileops").is_some(),
        "manifest missing fileops object: {}",
        manifest_text
    );

    // Basic sanity on fileops object
    let fileops = manifest.get("fileops").unwrap();
    assert!(fileops.get("dst").is_some(), "fileops missing dst");
    assert!(fileops.get("sha256").is_some(), "fileops missing sha256");
    assert!(fileops.get("bytes").is_some(), "fileops missing bytes");
    assert!(fileops.get("lines").is_some(), "fileops missing lines");
}
