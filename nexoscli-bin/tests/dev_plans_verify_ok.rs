mod helpers;
use helpers::*;
use serde_json::Value;

fn write(root: &std::path::Path, rel: &str, content: &str) {
    let p = root.join(rel);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

fn count_for(items: &Vec<Value>, name: &str) -> u64 {
    items.iter()
        .find(|it| it["name"].as_str() == Some(name))
        .and_then(|it| it["count"].as_u64())
        .unwrap_or(0)
}

#[test]
fn dev_plans_verify_ok() {
    let t = tmp();
    let root = t.path();

    // Arrange: plan lifecycle folders and files (happy path)
    std::fs::create_dir_all(root.join("docs/project/next")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/current")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/done")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/backlog")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/archived")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/paused")).unwrap();
    std::fs::create_dir_all(root.join("docs/project/dumped")).unwrap();

    // Exactly one current plan
    write(root, "docs/project/current/PLAN-50_example.md",
r#"---
status: in-progress
---
# Example current
"#);

    // Two next plans with distinct IDs (uppercase naming, no warnings)
    write(root, "docs/project/next/PLAN-51_next-a.md", "# next A\n");
    write(root, "docs/project/next/PLAN-52_next-b.md", "# next B\n");

    // Backlog: unnumbered idea (should be counted but no warnings)
    write(root, "docs/project/backlog/idea.md", "# idea\n");

    // Act
    let v = run_json(root, &["dev", "plans", "verify"]);
    // Uncomment for debugging:
    // eprintln!("{}", serde_json::to_string_pretty(&v).unwrap());

    // Assert
    assert!(v["data"]["ok"].as_bool().unwrap(), "verify should be OK");

    let items = v["data"]["items"].as_array().unwrap().clone();

    assert_eq!(count_for(&items, "current"), 1, "exactly one current plan required");
    assert_eq!(count_for(&items, "next"), 2, "two next plans expected");
    assert_eq!(count_for(&items, "backlog"), 1, "one backlog doc expected");
    // done may be zero in a new temp repo
    // informational folders should be present with counts (0 allowed)
    let _ = count_for(&items, "archived");
    let _ = count_for(&items, "paused");
    let _ = count_for(&items, "dumped");
}
