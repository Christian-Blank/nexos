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

fn find_item<'a>(items: &'a [Value], name: &str) -> &'a Value {
    items.iter().find(|it| it["name"].as_str() == Some(name)).expect("item by name")
}

fn has_substr(arr: &Value, needle: &str) -> bool {
    arr.as_array()
        .unwrap_or(&vec![])
        .iter()
        .any(|v| v.as_str().map(|s| s.contains(needle)).unwrap_or(false))
}

#[test]
fn dev_plans_verify_violations() {
    let t = tmp();
    let root = t.path();

    // Arrange folders
    for d in [
        "docs/project/next",
        "docs/project/current",
        "docs/project/done",
        "docs/project/backlog",
        "docs/project/archived",
        "docs/project/paused",
        "docs/project/dumped",
    ] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }

    // Violations:
    // - current/: two plans (must be exactly one)
    write(root, "docs/project/current/PLAN-10_a.md", "# current a\n");
    write(root, "docs/project/current/PLAN-11_b.md", "# current b\n");

    // - next/: duplicate plan_id (12) and a legacy lowercase 'plan-'
    write(root, "docs/project/next/PLAN-12_dup.md", "# next dup id 12\n");
    write(root, "docs/project/next/plan-12_legacy.md", "# legacy lowercase name\n");

    // - backlog/: numbered plan should not be here (warn)
    write(root, "docs/project/backlog/PLAN-99_bad-in-backlog.md", "# should be in next\n");

    // Act
    let v = run_json(root, &["dev", "plans", "verify"]);
    // eprintln!("{}", serde_json::to_string_pretty(&v).unwrap());

    // Assert overall
    assert_eq!(v["data"]["ok"].as_bool(), Some(false), "verify should fail with violations");

    let items = v["data"]["items"].as_array().expect("items array");

    // current: error "expected exactly 1 plan"
    let current = find_item(items, "current");
    assert_eq!(current["ok"].as_bool(), Some(false));
    assert!(has_substr(&current["errors"], "expected exactly 1 plan"));

    // next: duplicate plan_id 12; warning about legacy lowercase
    let next = find_item(items, "next");
    assert_eq!(next["ok"].as_bool(), Some(false), "duplicate ids should flip ok=false");
    assert!(has_substr(&next["errors"], "duplicate plan_id 12"), "should report duplicate plan_id");
    assert!(has_substr(&next["warnings"], "legacy lowercase name"), "should warn on lowercase 'plan-'");

    // backlog: warning numbered plan in backlog
    let backlog = find_item(items, "backlog");
    assert_eq!(backlog["ok"].as_bool(), Some(true), "backlog is warn-only");
    assert!(has_substr(&backlog["warnings"], "numbered plan in backlog"), "should warn numbered in backlog");
}
