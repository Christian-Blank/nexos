mod helpers; use helpers::*;

#[test]
fn dev_docs_build_generates_cli_reference() {
    let t = tmp();
    // make sure rules export doesn't fail during docs build
    let _ = run_json(t.path(), &["rules","init","--force"]);
    let v = run_json(t.path(), &["dev","docs","build"]);
    assert!(v["data"]["written"].as_array().unwrap().len() >= 2);
    assert!(t.path().join("docs/CLI_REFERENCE.md").exists());
    assert!(t.path().join("docs/RULES.md").exists());
}
