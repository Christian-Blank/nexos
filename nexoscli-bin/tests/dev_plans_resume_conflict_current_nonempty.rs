mod helpers;
use helpers::*;

fn write(root: &std::path::Path, rel: &str, content: &str) {
    let p = root.join(rel);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

fn exists(root: &std::path::Path, rel: &str) -> bool {
    root.join(rel).exists()
}

#[test]
fn dev_plans_resume_conflict_current_nonempty() {
    let t = tmp();
    let root = t.path();

    // Arrange folders
    std::fs::create_dir_all(root.join("docs/project/current")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/paused")).unwrap();

    // Seed one current plan (so current/ is non-empty)
    let cur = "PLAN-611_conflict-current.md";
    write(root, &format!("docs/project/current/{}", cur),
r#"---
status: in-progress
---
# Current
"#);

    // Seed one paused plan to resume
    let paused = "PLAN-612_to-resume.md";
    write(root, &format!("docs/project/paused/{}", paused),
r#"---
status: paused
---
# Paused
"#);

    // Act: resume should fail because current/ is non-empty
    let r = run_status_json(root, &["dev", "plans", "resume"]);

    // Assert: non-zero status and error envelope is produced
    assert_ne!(r.status, 0, "resume must fail when current/ is non-empty");

    // best-effort assertion on error code if present
    if let Some(ec) = r.json.get("error_code").and_then(|v| v.as_str()) {
        assert_eq!(ec, "plans.current.nonempty");
    }

    // Files unchanged
    assert!(exists(root, &format!("docs/project/current/{}", cur)));
    assert!(exists(root, &format!("docs/project/paused/{}", paused)));
}
