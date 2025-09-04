mod helpers;
use helpers::*;
use std::fs;
use std::path::Path;

fn read_first_ndjson_line(p: &Path) -> serde_json::Value {
    let txt = fs::read_to_string(p).expect("read ndjson");
    let mut it = txt.lines();
    let first = it.next().unwrap_or("");
    serde_json::from_str::<serde_json::Value>(first).expect("parse first json line")
}

#[test]
fn normalize_from_bag_writes_v03() {
    let t = tmp();

    // Produce a run and extract a TracePack (dir)
    let j = run_json(t.path(), &["issue", "new", "--title", "Normalize bag"]);
    let run_id = j["meta"]["run_id"].as_str().unwrap();
    let bag_dir = t.path().join("tracepack");
    let _ = run_json(
        t.path(),
        &["replay", "extract", "--run-id", run_id, "--out", bag_dir.to_str().unwrap()],
    );

    // Normalize in-place and update manifest
    let norm = run_json(
        t.path(),
        &[
            "replay",
            "normalize",
            "--to",
            "0.3",
            "--bag",
            bag_dir.to_str().unwrap(),
            "--update-manifest",
        ],
    );

    // Envelope data
    assert_eq!(
        norm["data"]["schema"].as_str().unwrap(),
        "nexos/events@0.3",
        "expected canonical schema tag"
    );
    let out_path = norm["data"]["out"].as_str().unwrap();
    assert!(
        out_path.ends_with("events.v0.3.ndjson"),
        "expected default out next to bag, got: {}",
        out_path
    );

    // File exists and first line matches v0.3 shape
    let out = Path::new(out_path);
    assert!(out.exists(), "output file not found: {}", out.display());
    let first = read_first_ndjson_line(out);
    assert_eq!(
        first.get("schema").and_then(|v| v.as_str()).unwrap_or(""),
        "nexos/events@0.3"
    );
    // Convenience attrs.source present and contains a dot
    let source = first
        .pointer("/attrs/source")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        source.contains('.'),
        "expected attrs.source to contain plugin.command, got: {:?}",
        source
    );

    // Manifest updated with events_v03 block
    let manifest_text = fs::read_to_string(bag_dir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    let ev03 = manifest.get("events_v03").cloned().unwrap_or(serde_json::json!({}));
    assert_eq!(
        ev03.get("schema").and_then(|v| v.as_str()).unwrap_or(""),
        "nexos/events@0.3",
        "manifest.events_v03.schema mismatch: {:?}",
        ev03
    );
    assert_eq!(
        ev03.get("dst").and_then(|v| v.as_str()).unwrap_or(""),
        "events.v0.3.ndjson",
        "manifest.events_v03.dst mismatch: {:?}",
        ev03
    );
    assert!(ev03.get("lines").and_then(|v| v.as_u64()).unwrap_or(0) > 0, "manifest.events_v03.lines");
    assert!(ev03.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0) > 0, "manifest.events_v03.bytes");
    assert!(
        ev03.get("sha256")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .starts_with("sha256:"),
        "manifest.events_v03.sha256 missing or malformed: {:?}",
        ev03
    );
}

#[test]
fn normalize_from_events_filters_run() {
    let t = tmp();

    // Create two runs into the shared events log
    let j1 = run_json(t.path(), &["issue", "new", "--title", "R1"]);
    let run_id1 = j1["meta"]["run_id"].as_str().unwrap().to_string();
    let _j2 = run_json(t.path(), &["issue", "new", "--title", "R2"]);

    // Paths
    let ev_path = events_path(t.path());
    assert!(ev_path.exists(), "expected shared events file at {}", ev_path.display());
    let all_events = read_events(t.path());
    assert!(all_events.len() >= 2, "expected at least two lines in shared log");

    // Normalize with run_id filter
    let out = t.path().join("events.v0.3.filtered.ndjson");
    let norm = run_json(
        t.path(),
        &[
            "replay",
            "normalize",
            "--to",
            "0.3",
            "--events",
            ev_path.to_str().unwrap(),
            "--run-id",
            &run_id1,
            "--out",
            out.to_str().unwrap(),
        ],
    );

    // Envelope
    assert_eq!(
        norm["data"]["schema"].as_str().unwrap(),
        "nexos/events@0.3"
    );
    assert_eq!(
        norm["data"]["run_id"].as_str().unwrap(),
        run_id1.as_str(),
        "expected run_id echoed in result"
    );
    let out_lines = norm["data"]["out_lines"].as_u64().unwrap_or(0);
    assert!(out_lines > 0, "expected some lines in filtered output");
    assert!(
        (out_lines as usize) < all_events.len(),
        "expected filtered lines fewer than total lines"
    );

    // Validate output contents
    let txt = fs::read_to_string(&out).expect("read filtered");
    for (i, line) in txt.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).expect(&format!("parse line {}", i + 1));
        assert_eq!(
            v.get("schema").and_then(|x| x.as_str()).unwrap_or(""),
            "nexos/events@0.3",
            "schema mismatch at line {}",
            i + 1
        );
        assert_eq!(
            v.get("run_id").and_then(|x| x.as_str()).unwrap_or(""),
            run_id1.as_str(),
            "run_id mismatch at line {}",
            i + 1
        );
    }
}
