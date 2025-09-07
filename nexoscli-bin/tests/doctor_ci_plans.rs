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

#[test]
fn doctor_ci_plans_ok() {
    let t = tmp();
    let root = t.path();

    // Arrange: valid plan lifecycle state
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

    // Exactly one current plan
    write(root, "docs/project/current/PLAN-50_example.md",
r#"---
status: in-progress
---
# Example current
"#);

    // A couple of distinct next plans
    write(root, "docs/project/next/PLAN-51_next-a.md", "# next A\n");
    write(root, "docs/project/next/PLAN-52_next-b.md", "# next B\n");

    // Backlog: unnumbered ok
    write(root, "docs/project/backlog/idea.md", "# idea\n");

    // Act: run doctor with only plans check (explicit → required)
    let sj = run_status_json(root, &["dev","doctor","--ci","--checks","plans","--output","json"]);

    // Assert exit 0 and ok=true
    assert_eq!(sj.status, 0, "doctor --ci --checks plans should pass");
    assert_eq!(sj.json["data"]["ok"].as_bool(), Some(true), "CI result should be ok");
    // ensure a plans check is present and ok
    let checks = sj.json["data"]["checks"].as_array().expect("checks array");
    let plans = checks.iter().find(|c| c["name"].as_str() == Some("plans")).expect("plans check present");
    assert_eq!(plans["ok"].as_bool(), Some(true));
}

#[test]
fn doctor_ci_plans_fails_on_violations() {
    let t = tmp();
    let root = t.path();

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

    // Violations: two current plans and duplicate id in next
    write(root, "docs/project/current/PLAN-10_a.md", "# cur a\n");
    write(root, "docs/project/current/PLAN-11_b.md", "# cur b\n");
    write(root, "docs/project/next/PLAN-12_x.md", "# next dup id 12\n");
    write(root, "docs/project/next/PLAN-12_y.md", "# next dup id 12 again\n");

    // Act: run doctor with only plans check
    let (code, out, _err) = run_raw(root, &["dev","doctor","--ci","--checks","plans","--output","json"]);
    assert!(code != 0, "doctor --ci --checks plans should fail on violations");
    let v: Value = serde_json::from_str(&out).expect("json");
    assert!(v["errors"].as_array().unwrap().len() >= 1, "errors should be reported");
}
