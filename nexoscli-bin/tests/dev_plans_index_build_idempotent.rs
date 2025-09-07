mod helpers;
use helpers::*;
use serde_json::Value;
use std::path::Path;

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

fn readme_with_auto() -> &'static str {
    r#"# README

## Plans Index

<!-- BEGIN: AUTO (id=plans-index) -->
- None
<!-- END: AUTO -->
"#
}

fn touched_contains(touched: &Value, rel: &str) -> bool {
    touched
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .any(|v| v.as_str().map(|s| s.ends_with(rel)).unwrap_or(false))
}

#[test]
fn dev_plans_index_build_is_idempotent() {
    let t = tmp();
    let root = t.path();

    // Arrange: lifecycle folders and minimal READMEs with AUTO block
    for d in ["next","current","done","backlog","archived","paused","dumped"] {
        let base = format!("docs/project/{}/README.md", d);
        write(root, &base, readme_with_auto());
    }

    // Place some plan files
    write(root, "docs/project/current/PLAN-50_cur.md", "# current\n");
    write(root, "docs/project/next/PLAN-51_next-a.md", "# next a\n");
    write(root, "docs/project/next/PLAN-52_next-b.md", "# next b\n");
    write(root, "docs/project/done/PLAN-40_done.md", "# done\n");
    // Backlog: unnumbered listed; numbered should be excluded from backlog index
    write(root, "docs/project/backlog/idea-a.md", "# idea\n");
    write(root, "docs/project/backlog/PLAN-99_should-not-list.md", "# numbered (exclude)\n");

    // Act 1: build indexes
    let v1 = run_json(root, &["dev","plans","index","build"]);
    // eprintln!("{}", serde_json::to_string_pretty(&v1).unwrap());

    assert_eq!(v1["data"]["ok"].as_bool(), Some(true));
    let touched1 = &v1["data"]["touched"];
    // All folders have READMEs and should have been updated at least once
    for rel in [
        "docs/project/next/README.md",
        "docs/project/current/README.md",
        "docs/project/done/README.md",
        "docs/project/backlog/README.md",
        "docs/project/archived/README.md",
        "docs/project/paused/README.md",
        "docs/project/dumped/README.md",
    ] {
        assert!(touched_contains(touched1, rel), "expected first run to touch {}", rel);
    }
    // No warnings (we provided all READMEs)
    assert_eq!(v1["data"]["warnings"].as_array().unwrap().len(), 0);

    // Act 2: second run should be idempotent (no further changes)
    let v2 = run_json(root, &["dev","plans","index","build"]);
    // eprintln!("{}", serde_json::to_string_pretty(&v2).unwrap());

    assert_eq!(v2["data"]["ok"].as_bool(), Some(true));
    let touched2 = v2["data"]["touched"].as_array().unwrap();
    assert!(touched2.is_empty(), "second run must be idempotent (no touched files)");
}
