mod helpers; 
use helpers::*;

#[test]
fn bundle_build_and_verify_against_current() {
    let t = tmp();

    // Prepare minimal repo: issues + docs/exec_graph.yaml
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    // two local issues
    let _ = run_json(t.path(), &["issue","new","--title","A"]);
    let _ = run_json(t.path(), &["issue","new","--title","B"]);
    // graph
    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let graph_yaml = r#"
id: EG-test
nodes:
  - id: A
    deps: []
  - id: B
    deps: [A]
"#;
    std::fs::write(t.path().join("docs/exec_graph.yaml"), graph_yaml.trim()).unwrap();

    // build
    let v = run_json(t.path(), &["bundle","build","EG-test","--include","issues,docs"]);
    let bpath = v["data"]["path"].as_str().unwrap();
    assert!(std::path::Path::new(bpath).exists());

    // verify (archive itself)
    let v2 = run_json(t.path(), &["bundle","verify", bpath]);
    assert!(v2["data"]["archive_ok"].as_bool().unwrap());

    // verify against current filesystem
    let v3 = run_json(t.path(), &["bundle","verify", bpath, "--against", "."]);
    assert!(v3["data"]["against_ok"].as_bool().unwrap());

    // corrupt a file and fail against
    // change one issue file to break checksum
    let issues_dir = t.path().join(".nexoscli/issues");
    let one = std::fs::read_dir(&issues_dir).unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().map(|x| x=="json").unwrap_or(false))
        .unwrap().path();
    std::fs::write(&one, br#"{"id":"tampered"}"#).unwrap();

    let (code, out, _err) = run_raw(t.path(), &["bundle","verify", bpath, "--against", ".", "--output", "json"]);
    assert!(code != 0, "verify --against should fail after tamper");
    let v4: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v4["data"]["against_ok"].as_bool() == Some(false));

    export_events(t.path(), "bundle_build_verify");
}
