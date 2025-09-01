mod helpers; use helpers::*;

#[test]
fn doctor_ci_includes_rules_check() {
    let t = tmp();
    // rules missing -> check still informational, ok=true
    let v = run_json(t.path(), &["dev","doctor","--ci","--checks","rules"]);
    assert!(v["data"]["ok"].as_bool().unwrap());

    // now init rules and require them
    let _ = run_json(t.path(), &["rules","init","--force"]);
    let v2 = run_json(t.path(), &["dev","doctor","--ci","--checks","rules","--require","rules"]);
    assert!(v2["data"]["ok"].as_bool().unwrap());
}
