mod helpers; use helpers::*;
use serde_json::json;

#[test]
fn graph_check_ok_and_impact() {
    let t = tmp();

    // Create a simple acyclic graph in temp repo
    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let g = serde_yaml::to_string(&json!({
        "id": "EG-test",
        "nodes": [
            { "id": "A", "deps": [] },
            { "id": "B", "deps": ["A"] },
            { "id": "C", "deps": ["B"] },
            { "id": "D", "deps": ["A"] }
        ]
    })).unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g).unwrap();

    let v = run_json(t.path(), &["graph","check","--file","docs/exec_graph.yaml"]);
    assert!(v["errors"].as_array().unwrap().is_empty());
    assert!(v["data"]["acyclic"].as_bool().unwrap());

    let v = run_json(t.path(), &["graph","impact","A","--file","docs/exec_graph.yaml"]);
    let impacted = v["data"]["impacted"].as_array().unwrap();
    for want in ["B","C","D"] {
        assert!(impacted.iter().any(|x| x.as_str()==Some(want)), "missing {want}");
    }

    export_events(t.path(), "graph_e2e_ok");
}

#[test]
fn graph_check_cycle_detected() {
    let t = tmp();

    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let g = serde_yaml::to_string(&json!({
        "id": "EG-cycle",
        "nodes": [
            { "id": "A", "deps": ["B"] },
            { "id": "B", "deps": ["A"] }
        ]
    })).unwrap();
    std::fs::write(t.path().join("docs/exec_graph.yaml"), g).unwrap();

    let (code, out, _err) = run_raw(t.path(), &["graph","check","--file","docs/exec_graph.yaml","--output","json"]);
    assert!(code != 0, "should fail on cycle");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["errors"].as_array().unwrap().len() >= 1);

    export_events(t.path(), "graph_e2e_cycle");
}
