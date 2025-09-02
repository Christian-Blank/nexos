mod helpers;

#[test]
fn workflow_phase18_visibility_and_filters() {
    let temp_dir = helpers::tmp();
    let tmp = temp_dir.path();

    // Pre-flight
    helpers::run_ok(tmp, &["dev", "init", "--yes"]);

    // Init spec and run
    helpers::run_ok(tmp, &["workflow", "init", "--id", "WF-demo"]);
    let run = helpers::run_json(
        tmp,
        &[
            "workflow",
            "run",
            "--spec",
            ".nexoscli/workflows/WF-demo/spec.yaml",
        ],
    );
    assert_eq!(
        run["data"]["failed"].as_u64().unwrap_or(1),
        0,
        "run should not fail"
    );

    // Query visibility
    let q = helpers::run_json(tmp, &["workflow", "query", "--id", "WF-demo", "--name", "status"]);
    assert_eq!(
        q["data"]["id"].as_str().unwrap(),
        "WF-demo",
        "visibility should exist and match id"
    );

    // List by default SA
    let l1 = helpers::run_json(tmp, &["workflow", "list", "--filter", "task_queue=nexos"]);
    assert!(
        l1["data"].as_array().is_some(),
        "list should return an array"
    );

    // Upsert search attributes
    let up = helpers::run_json(
        tmp,
        &[
            "workflow",
            "upsert-sa",
            "--id",
            "WF-demo",
            "--attrs",
            "owner=christian",
            "--attrs",
            "priority=high",
        ],
    );
    assert_eq!(
        up["data"]["search_attributes"]["owner"].as_str().unwrap(),
        "christian",
        "owner SA should be upserted"
    );

    // List by new SA
    let l2 = helpers::run_json(tmp, &["workflow", "list", "--filter", "owner=christian"]);
    let arr = l2["data"].as_array().expect("array");
    assert!(
        arr.iter().any(|v| v["id"].as_str() == Some("WF-demo")),
        "filtered list should contain WF-demo"
    );
}

#[test]
fn workflow_phase18_signal_enqueue() {
    let temp_dir = helpers::tmp();
    let tmp = temp_dir.path();

    helpers::run_ok(tmp, &["dev", "init", "--yes"]);
    helpers::run_ok(tmp, &["workflow", "init", "--id", "WF-sig"]);

    // Enqueue a signal (pause)
    let s = helpers::run_json(
        tmp,
        &[
            "workflow",
            "signal",
            "--id",
            "WF-sig",
            "--name",
            "pause",
            "--payload",
            "{}",
        ],
    );
    assert_eq!(
        s["data"]["name"].as_str().unwrap(),
        "pause",
        "signal name should echo"
    );
    assert!(
        s["data"]["file"].as_str().unwrap().contains("WF-sig"),
        "signal file path should include workflow id"
    );
}
