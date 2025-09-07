mod helpers;
use helpers::*;
use serde_json::Value;
use std::fs;

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
fn dev_plans_pause_resume_ok() {
    let t = tmp();
    let root = t.path();

    // Arrange folders
    std::fs::create_dir_all(root.join("docs/project/next")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/current")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/paused")).unwrap();

    // Seed one current plan
    let fname = "PLAN-610_pause-resume.md";
    write(root, &format!("docs/project/current/{}", fname),
r#"---
status: in-progress
---
# Pause/Resume Test
"#);

    // Act: pause (no --plan uses singleton in current/)
    let v_pause = run_json(root, &["dev", "plans", "pause"]);
    // eprintln!("{}", serde_json::to_string_pretty(&v_pause).unwrap());

    // Assert: moved to paused/
    let moved_to_pause = v_pause["data"]["moved_to"].as_str().unwrap();
    assert!(moved_to_pause.ends_with(&format!("docs/project/paused/{}", fname)));
    assert!(exists(root, &format!("docs/project/paused/{}", fname)));
    assert!(!exists(root, &format!("docs/project/current/{}", fname)));

    // Act: resume (no --plan uses singleton in paused/)
    let v_resume = run_json(root, &["dev", "plans", "resume"]);

    // Assert: moved back to current/, reconciliation present
    let moved_to_resume = v_resume["data"]["moved_to"].as_str().unwrap();
    assert!(moved_to_resume.ends_with(&format!("docs/project/current/{}", fname)));
    assert!(exists(root, &format!("docs/project/current/{}", fname)));
    assert!(!exists(root, &format!("docs/project/paused/{}", fname)));

    // Check reconciliation summary present and well-formed
    let recon = &v_resume["data"]["reconciliation"];
    assert!(recon.is_object(), "reconciliation must be an object");
    assert!(recon.get("plans_ok").is_some());
    assert!(recon.get("repo_ok").is_some());
}
