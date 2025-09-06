mod helpers;
use helpers::*;
use std::fs;

#[test]
fn dev_schemas_export_writes_all_files() {
    let t = tmp();

    // export schemas into contracts/schemas inside the temp project dir
    let v = run_json(t.path(), &["dev","schemas","export","--out","contracts/schemas"]);
    assert_eq!(v["data"]["out"], "contracts/schemas");

    // files exist
    let out = t.path().join("contracts/schemas");
    assert!(out.join("event_v0.3.schema.json").exists(), "missing event_v0.3.schema.json");
    assert!(out.join("term_record_v0.1.schema.json").exists(), "missing term_record_v0.1.schema.json");
    assert!(out.join("file_watch_event_v0.1.schema.json").exists(), "missing file_watch_event_v0.1.schema.json");
    assert!(out.join("bag_manifest_v0.2.schema.json").exists(), "missing bag_manifest_v0.2.schema.json");

    // sanity: JSON parses for each exported file
    for f in [
        "event_v0.3.schema.json",
        "term_record_v0.1.schema.json",
        "file_watch_event_v0.1.schema.json",
        "bag_manifest_v0.2.schema.json",
    ] {
        let txt = fs::read_to_string(out.join(f)).expect("read schema");
        let _: serde_json::Value = serde_json::from_str(&txt).expect("parse schema json");
    }
}
