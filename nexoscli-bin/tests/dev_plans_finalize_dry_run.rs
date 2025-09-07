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
fn dev_plans_finalize_dry_run() {
    let t = tmp();
    let root = t.path();

    // Arrange lifecycle folders
    std::fs::create_dir_all(root.join("docs/project/current")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/done")).unwrap();

    // Seed one current plan without task_id to avoid side-effects
    let fname = "PLAN-630_finalize.md";
    write(root, &format!("docs/project/current/{}", fname),
r#"---
status: in-progress
---
# Finalize Dry Run
"#);

    // Act: finalize in dry-run mode
    let v = run_json(root, &["dev", "plans", "finalize", "--dry-run"]);

    // Assert: envelope shows target moved_to path under done/
    let moved_to = v["data"]["moved_to"].as_str().unwrap();
    assert!(moved_to.ends_with(&format!("docs/project/done/{}", fname)));

    // Assert: file NOT moved in dry-run
    assert!(exists(root, &format!("docs/project/current/{}", fname)));
    assert!(!exists(root, &format!("docs/project/done/{}", fname)));

    // Assert: frontmatter preview includes status done and completed_at
    let fm = &v["data"]["frontmatter"];
    assert_eq!(fm["status"].as_str().unwrap(), "done");
    assert!(fm.get("completed_at").is_some(), "completed_at should be set in dry-run preview");
    // implemented_in may be empty if not a git repo; just assert it is a string key
    assert!(fm.get("implemented_in").is_some(), "implemented_in should be present (may be empty in temp repo)");
}
