mod helpers;
use helpers::*;
use serde_json::json;

#[test]
fn doctor_ci_passes_then_fails_on_cycle() {
    let t = tmp();
    // prepare repo
    let _ = run_json(t.path(), &["dev", "init", "--yes"]);

    // valid graph
    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let g = serde_yaml::to_string(&json!({
        "id": "EG-ok",
        "nodes": [
            { "id": "A", "deps": [] },
            { "id": "B", "deps": ["A"] }
        ]
    }))
    .unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g).unwrap();

    // structure check should be OK for empty plugins dir (no errors)
    let v = run_json(t.path(), &["dev", "plugins", "check"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));

    // doctor --ci OK
    let v = run_json(t.path(), &["dev", "doctor", "--ci"]);
    assert!(
        v["data"]["ok"].as_bool().unwrap(),
        "doctor --ci should pass"
    );

    // introduce a cycle
    let g2 = serde_yaml::to_string(&json!({
        "id": "EG-bad",
        "nodes": [
            { "id": "A", "deps": ["B"] },
            { "id": "B", "deps": ["A"] }
        ]
    }))
    .unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g2).unwrap();

    // doctor --ci must fail now
    let (code, out, _err) = run_raw(t.path(), &["dev", "doctor", "--ci", "--output", "json"]);
    assert!(code != 0, "doctor --ci should fail");
    let v2: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v2["errors"].as_array().unwrap().len() >= 1);
}
