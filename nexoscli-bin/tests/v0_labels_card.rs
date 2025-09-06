mod helpers;

use std::{fs, path::PathBuf};

fn last_run_id_from_events(tmp: &std::path::Path) -> Option<String> {
    let evs = helpers::read_events(tmp);
    // Prefer the most recent term.run event
    for ev in evs.iter().rev() {
        let plugin = ev.get("plugin").and_then(|v| v.as_str());
        let command = ev.get("command").and_then(|v| v.as_str());
        if plugin == Some("term") && command == Some("run") {
            if let Some(r) = ev.get("run_id").and_then(|v| v.as_str()) {
                return Some(r.to_string());
            }
        }
    }
    // Fallback: any event with a run_id
    evs.iter()
        .rev()
        .filter_map(|ev| ev.get("run_id").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .next()
}

#[test]
fn v0_labels_card_e2e() {
    let tmp = helpers::tmp();
    let t = tmp.path();

    // init repo in tmp
    helpers::run_ok(t, &["dev", "init", "--yes"]);

    // create a small activity to ensure events for a fresh run_id
    helpers::run_ok(t, &["term", "run", "--", "bash", "-lc", "echo hi >> demo.txt"]);

    // extract run_id from events
    let run_id = last_run_id_from_events(t).expect("run_id");

    // extract a bag
    helpers::run_ok(t, &["replay", "extract", "--run-id", &run_id, "--out", "bag1"]);

    // annotate with labels (PLAN-42)
    let res = helpers::run_json(
        t,
        &[
            "replay",
            "annotate",
            "--bag",
            "bag1",
            "--labels",
            "exp_id=EXP-001",
            "--labels",
            "treatment=A",
            "--labels",
            "env=dev",
        ],
    );

    // verify envelope semantics
    assert_eq!(
        res.get("meta").and_then(|m| m.get("plugin")).and_then(|v| v.as_str()),
        Some("replay")
    );
    assert_eq!(
        res.get("meta").and_then(|m| m.get("command")).and_then(|v| v.as_str()),
        Some("annotate.labels")
    );

    // manifest.labels contains provided pairs
    let man_path = PathBuf::from(t).join("bag1").join("manifest.json");
    let man_raw = fs::read(&man_path).expect("read manifest");
    let manifest: serde_json::Value = serde_json::from_slice(&man_raw).expect("parse manifest");
    let labels = manifest
        .get("labels")
        .and_then(|v| v.as_object())
        .expect("labels object");
    assert_eq!(labels.get("exp_id").and_then(|v| v.as_str()), Some("EXP-001"));
    assert_eq!(labels.get("treatment").and_then(|v| v.as_str()), Some("A"));
    assert_eq!(labels.get("env").and_then(|v| v.as_str()), Some("dev"));

    // events.ndjson has attrs.labels merged
    let ev_path_root = PathBuf::from(t).join("bag1").join("events.ndjson");
    let ev_path_topics = PathBuf::from(t).join("bag1").join("topics").join("events.ndjson");
    let ev_path = if ev_path_root.exists() { ev_path_root } else { ev_path_topics };
    let ev_txt = fs::read_to_string(&ev_path).expect("read events.ndjson");
    let first_line = ev_txt
        .lines()
        .find(|l| !l.trim().is_empty())
        .expect("non-empty events");
    let row: serde_json::Value = serde_json::from_str(first_line).expect("parse row json");
    let lobj = row
        .pointer("/attrs/labels")
        .and_then(|v| v.as_object())
        .expect("attrs.labels");
    assert_eq!(lobj.get("exp_id").and_then(|v| v.as_str()), Some("EXP-001"));
    assert_eq!(lobj.get("treatment").and_then(|v| v.as_str()), Some("A"));
    assert_eq!(lobj.get("env").and_then(|v| v.as_str()), Some("dev"));

    // verify with fix-readme (PLAN-43)
    helpers::run_ok(t, &["replay", "verify", "--path", "bag1", "--fix-readme"]);

    // README.md exists and includes expected sections
    let readme_path = PathBuf::from(t).join("bag1").join("README.md");
    assert!(readme_path.exists(), "README.md not found in bag");
    let readme = fs::read_to_string(&readme_path).expect("read README.md");
    for needle in [
        "## Contents & Schemas",
        "## Counts",
        "## Labels",
        "## Tooling",
        "## Load in Python (Pandas)",
        "## Load in DuckDB (SQL)",
    ] {
        assert!(
            readme.contains(needle),
            "README.md missing section: {}",
            needle
        );
    }
}
