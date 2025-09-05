mod helpers;
use helpers::*;

#[test]
fn dev_doctor_ci_equals_form_flags() {
    let t = tmp();

    // Initialize a fresh workspace for the test
    let _ = run_json(t.path(), &["dev", "init", "--yes"]);

    // Provide a minimal valid exec_graph.yaml so graph check can pass
    std::fs::create_dir_all(t.path().join("docs")).unwrap();
    let graph_yaml = r#"id: EG-ok
nodes:
  - id: A
    deps: []
  - id: B
    deps: ["A"]
"#;
    std::fs::write(t.path().join("docs/exec_graph.yaml"), graph_yaml).unwrap();

    // Run doctor with equals-form CSV flags (copy/paste safe)
    let v = run_json(
        t.path(),
        &[
            "dev",
            "doctor",
            "--ci",
            "--checks",
            "graph,plugins,config,events,tools",
            "--require",
            "plugins,graph",
        ],
    );

    // Expect overall ok=true in the envelope data
    assert!(
        v["data"]["ok"].as_bool().unwrap_or(false),
        "doctor CI equals-form flags should succeed, got: {}",
        v
    );
}
