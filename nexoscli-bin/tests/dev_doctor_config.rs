mod helpers; use helpers::*;

#[test]
fn doctor_ready_and_config_default_output_yaml() {
    let t = tmp();

    // Ensure dotdir exists, then doctor
    let _ = run_json(t.path(), &["dev","init","--yes"]);
    
    // Create redaction config required by doctor
    let redact_path = t.path().join(".nexoscli").join("redact.yaml");
    std::fs::write(&redact_path, "patterns: []").unwrap();
    
    let v = run_json(t.path(), &["dev","doctor"]);
    assert!(v["data"]["ready"].as_bool().unwrap());

    // Change default output to YAML via config
    std::fs::write(
        t.path().join(".nexoscli").join("config.toml"),
        r#"[output]
default = "yaml"
"#).unwrap();

    // No --output flag -> YAML rendering
    let (_code, out, _err) = run_raw(t.path(), &["issue","new","--title","YAML default"]);
    assert!(out.trim_start().starts_with("data:"), "expected YAML, got:\n{}", out);

    export_events(t.path(), "dev_doctor_config");
}
