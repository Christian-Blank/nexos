mod helpers; use helpers::*;
use std::fs;

#[test]
fn doctor_ci_reports_cline_info_and_can_require_it() {
    let t = tmp();

    // No storage -> info only, still OK (no --require)
    let v = run_json(t.path(), &["dev","doctor","--ci","--checks","cline"]);
    assert!(v["data"]["ok"].as_bool().unwrap());

    // Now create a fake storage and require cline
    let root = t.path().join("globalStorage").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&root).unwrap();
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", t.path().join("globalStorage").to_str().unwrap());

    // Should pass even when required (dry-run import succeeds)
    let v2 = run_json(t.path(), &["dev","doctor","--ci","--checks","cline","--require","cline"]);
    assert!(v2["data"]["ok"].as_bool().unwrap());
}
