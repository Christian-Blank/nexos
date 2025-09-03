mod helpers;
use helpers::*;
use std::path::Path;

#[test]
fn bag_build_verify_info_and_cat() {
    let t = tmp();

    // Generate activity to produce events (ledger v0.3 enabled by default)
    let j = run_json(t.path(), &["issue", "new", "--title", "Bag test"]);
    let run_id = j["meta"]["run_id"].as_str().unwrap();

    // Build a directory bag
    let outdir = t.path().join("bagdir");
    let b = run_json(
        t.path(),
        &[
            "replay",
            "bag",
            "build",
            "--run-id",
            run_id,
            "--out",
            outdir.to_str().unwrap(),
        ],
    );
    assert_eq!(b["data"]["format"], "dir");
    assert!(outdir.join("manifest.json").exists());
    assert!(outdir.join("topics").join("events.ndjson").exists());
    assert!(outdir.join("README.md").exists());
    // Chain should verify for v0.3 events
    assert_eq!(b["data"]["chain_ok"].as_bool().unwrap_or(false), true);

    // Verify
    let v = run_json(
        t.path(),
        &["replay", "bag", "verify", "--bag", outdir.to_str().unwrap()],
    );
    assert_eq!(v["data"]["ok"], true);
    assert!(v["data"]["count"].as_u64().unwrap() >= 1);

    // Info
    let i = run_json(
        t.path(),
        &["replay", "bag", "info", "--bag", outdir.to_str().unwrap()],
    );
    assert_eq!(i["data"]["ok"], true);
    assert_eq!(i["data"]["format"], "dir");
    assert!(i["data"]["counts"]["events"].as_u64().unwrap() >= 1);

    // Cat (unbounded)
    let c = run_json(
        t.path(),
        &[
            "replay",
            "bag",
            "cat",
            "--bag",
            outdir.to_str().unwrap(),
            "--topic",
            "events",
        ],
    );
    assert_eq!(c["data"]["ok"], true);
    assert!(c["data"]["lines"].as_u64().unwrap() >= 1);
    assert!(c["data"]["bytes"].as_u64().unwrap() >= 1);

    // TGZ variant
    let tgz = t.path().join("bag.tgz");
    let b2 = run_json(
        t.path(),
        &[
            "replay",
            "bag",
            "build",
            "--run-id",
            run_id,
            "--out",
            tgz.to_str().unwrap(),
        ],
    );
    assert_eq!(b2["data"]["format"], "tgz");
    assert!(Path::new(&tgz).exists());

    let v2 = run_json(
        t.path(),
        &["replay", "bag", "verify", "--bag", tgz.to_str().unwrap()],
    );
    assert_eq!(v2["data"]["ok"], true);

    let i2 = run_json(
        t.path(),
        &["replay", "bag", "info", "--bag", tgz.to_str().unwrap()],
    );
    assert_eq!(i2["data"]["format"], "tgz");

    let c2 = run_json(
        t.path(),
        &[
            "replay",
            "bag",
            "cat",
            "--bag",
            tgz.to_str().unwrap(),
            "--topic",
            "events",
            "--lines",
            "5",
        ],
    );
    assert!(c2["data"]["lines"].as_u64().unwrap() <= 5);
}
