mod helpers; use helpers::*;
use std::fs;

#[test]
fn rules_init_lint_export() {
    let t = tmp();
    // init
    let _ = run_json(t.path(), &["rules","init","--force"]);
    assert!(t.path().join("rules/pack/core/RULE-use-term-run.md").exists());

    // lint
    let v = run_json(t.path(), &["rules","lint"]);
    assert!(v["data"]["ok"].as_bool().unwrap());

    // export for cline
    let v2 = run_json(t.path(), &["rules","export","--for","cline","--out",".clinerules/compiled.md"]);
    assert!(t.path().join(".clinerules/compiled.md").exists());
    let body = fs::read_to_string(t.path().join(".clinerules/compiled.md")).unwrap();
    assert!(body.contains("RULE-use-term-run"));

    // export for humans (docs)
    let _ = run_json(t.path(), &["rules","export","--for","human","--out","docs/RULES.md"]);
    assert!(t.path().join("docs/RULES.md").exists());
}
