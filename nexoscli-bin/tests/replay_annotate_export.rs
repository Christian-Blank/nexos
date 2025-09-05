mod helpers;
use helpers::*;
use std::fs;

#[test]
fn replay_annotate_and_export() {
    let t = tmp();

    // Initialize a repo so file operations make sense
    run_json(t.path(), &["dev","init","--yes"]);

    // Run a command that changes files and emits fs-topic so fileops will exist in extract
    // Ensures bag/fileops/fileops.ndjson is present after extraction (same pattern as replay_export_csv.rs)
    let j = run_json(
        t.path(),
        &[
            "term","run","--fs-topic","sh","-lc",
            "git init -q && git config user.email t@e && git config user.name T && echo one > a.txt && echo two >> a.txt && mv a.txt b.txt && rm -f b.txt"
        ]
    );
    let run_id = j["meta"]["run_id"].as_str().expect("run_id");

    // Extract a TracePack directory bag
    let bag = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id", run_id, "--out", bag.to_str().unwrap()]);

    // Sanity: events and fileops should exist
    assert!(bag.join("events.ndjson").exists(), "expected events.ndjson in TracePack");
    assert!(bag.join("fileops").join("fileops.ndjson").exists(), "expected fileops/fileops.ndjson in TracePack");

    // Add annotations: one for first event row, one for first fileop row
    let _a1 = run_json(t.path(), &[
        "replay","annotate","add",
        "--bag", bag.to_str().unwrap(),
        "--target","events",
        "--seq","0",
        "--label","slow","--label","investigate",
        "--note","p90 high",
        "--author","tester",
        "--confidence","0.8"
    ]);
    let _a2 = run_json(t.path(), &[
        "replay","annotate","add",
        "--bag", bag.to_str().unwrap(),
        "--target","fileops",
        "--seq","0",
        "--label","write-hot"
    ]);

    // List annotations
    let vlist = run_json(t.path(), &["replay","annotate","list","--bag",bag.to_str().unwrap()]);
    assert_eq!(vlist["data"]["count"].as_u64().unwrap(), 2, "expected 2 annotations listed");

    // Export annotations CSV only
    let _v_ann = run_json(t.path(), &["replay","export","--bag",bag.to_str().unwrap(),"--what","annotations"]);
    let ann_csv = bag.join("exports").join("csv").join("annotations.csv");
    assert!(ann_csv.exists(), "missing {}", ann_csv.display());
    let ann_body = fs::read_to_string(&ann_csv).unwrap();
    assert!(ann_body.lines().count() >= 2, "annotations.csv should have header + at least one data row");
    assert!(ann_body.lines().next().unwrap().contains("target_kind,target_seq"), "annotations.csv header should contain target_* fields");

    // Export events joined with annotations
    let _v_e = run_json(t.path(), &["replay","export","--bag",bag.to_str().unwrap(),"--what","events","--join","annotations"]);
    let e_ann = bag.join("exports").join("csv").join("events_annotated.csv");
    assert!(e_ann.exists(), "missing {}", e_ann.display());
    let e_body = fs::read_to_string(&e_ann).unwrap();
    let e_hdr = e_body.lines().next().unwrap_or("");
    assert!(e_hdr.contains("ann_labels") && e_hdr.contains("ann_count"), "events_annotated.csv header should contain ann_* columns");
    assert!(e_body.contains("investigate"), "expected label 'investigate' to appear in events_annotated.csv");

    // Export fileops joined with annotations
    let _v_f = run_json(t.path(), &["replay","export","--bag",bag.to_str().unwrap(),"--what","fileops","--join","annotations"]);
    let f_ann = bag.join("exports").join("csv").join("fileops_annotated.csv");
    assert!(f_ann.exists(), "missing {}", f_ann.display());
    let f_body = fs::read_to_string(&f_ann).unwrap();
    let f_hdr = f_body.lines().next().unwrap_or("");
    assert!(f_hdr.contains("ann_labels") && f_hdr.contains("ann_count"), "fileops_annotated.csv header should contain ann_* columns");
}
