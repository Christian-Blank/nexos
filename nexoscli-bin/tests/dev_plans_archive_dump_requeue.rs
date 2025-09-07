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
fn dev_plans_archive_dump_ok() {
    let t = tmp();
    let root = t.path();

    // Arrange folders
    std::fs::create_dir_all(root.join("docs/project/next")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/archived")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/dumped")).unwrap();

    // Seed one plan in next/
    let plan_a = "PLAN-620_for-archive.md";
    write(root, &format!("docs/project/next/{}", plan_a),
r#"---
status: planning
---
# Archive Me
"#);

    // Act: archive with reason
    let v_arch = run_json(root, &[
        "dev","plans","archive",
        "--plan",&format!("docs/project/next/{}", plan_a),
        "--reason","obsolete"
    ]);

    // Assert: moved to archived/
    let moved_to = v_arch["data"]["moved_to"].as_str().unwrap();
    assert!(moved_to.ends_with(&format!("docs/project/archived/{}", plan_a)));
    assert!(exists(root, &format!("docs/project/archived/{}", plan_a)));
    assert!(!exists(root, &format!("docs/project/next/{}", plan_a)));
    // Rationale present in front-matter body
    let arch_txt = std::fs::read_to_string(root.join(format!("docs/project/archived/{}", plan_a))).unwrap();
    assert!(arch_txt.contains("rationale:"), "archive should add rationale header when --reason is provided");

    // Seed one plan to dump
    let plan_d = "PLAN-621_for-dump.md";
    write(root, &format!("docs/project/next/{}", plan_d),
r#"---
status: planning
---
# Dump Me
"#);

    // Act: dump with reason
    let v_dump = run_json(root, &[
        "dev","plans","dump",
        "--plan",&format!("docs/project/next/{}", plan_d),
        "--reason","invalid"
    ]);

    // Assert: moved to dumped/
    let moved_to_d = v_dump["data"]["moved_to"].as_str().unwrap();
    assert!(moved_to_d.ends_with(&format!("docs/project/dumped/{}", plan_d)));
    assert!(exists(root, &format!("docs/project/dumped/{}", plan_d)));
    assert!(!exists(root, &format!("docs/project/next/{}", plan_d)));
    let dump_txt = std::fs::read_to_string(root.join(format!("docs/project/dumped/{}", plan_d))).unwrap();
    assert!(dump_txt.contains("rationale:"), "dump should add rationale header when --reason is provided");
}

#[test]
fn dev_plans_requeue_ok_warns_numbered() {
    let t = tmp();
    let root = t.path();

    // Arrange folders
    std::fs::create_dir_all(root.join("docs/project/paused")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/backlog")).unwrap();

    // Seed a numbered paused plan
    let fname = "PLAN-622_requeue.md";
    write(root, &format!("docs/project/paused/{}", fname),
r#"---
status: paused
---
# Paused numbered
"#);

    // Act: requeue (no --plan, singleton in paused/)
    let v = run_json(root, &["dev","plans","requeue"]);

    // Assert: moved to backlog and warnings mention numbered plan
    let moved_to = v["data"]["moved_to"].as_str().unwrap();
    assert!(moved_to.ends_with(&format!("docs/project/backlog/{}", fname)));
    assert!(exists(root, &format!("docs/project/backlog/{}", fname)));
    assert!(!exists(root, &format!("docs/project/paused/{}", fname)));

    let warns = v["data"]["warnings"].as_array().cloned().unwrap_or_default();
    let has_numbered_warn = warns.iter().filter_map(|w| w.as_str()).any(|s| s.to_lowercase().contains("numbered"));
    assert!(has_numbered_warn, "requeue should warn about numbered backlog entries");
}
