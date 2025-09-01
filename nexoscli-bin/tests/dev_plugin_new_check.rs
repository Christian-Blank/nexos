mod helpers; use helpers::*;

#[test]
fn scaffold_plugin_and_check_structure() {
    let t = tmp();

    // Scaffold without registration (safe for a temp repo)
    let v = run_json(t.path(), &["dev","plugin","new","--name","demo","--desc","Demo plugin","--yes"]);
    assert_eq!(v["data"]["registered"], false);
    let created = v["data"]["created_paths"].as_array().unwrap();
    assert!(created.iter().any(|p| p.as_str().unwrap().ends_with("plugins/demo/Cargo.toml")));
    assert!(created.iter().any(|p| p.as_str().unwrap().ends_with("plugins/demo/src/lib.rs")));
    assert!(created.iter().any(|p| p.as_str().unwrap().ends_with("plugins/demo/README.md")));

    // Structure check
    let v = run_json(t.path(), &["dev","plugins","check","--fix-readmes"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));

    export_events(t.path(), "dev_plugin_new_check");
}
