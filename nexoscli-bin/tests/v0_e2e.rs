mod helpers;
use helpers::*;
use std::{fs, path::Path};

#[test]
fn v0_e2e_extract_verify_doctor_profile_v0() {
    let t = tmp();

    // 1) Create some activity to generate events (and optionally fs.topic)
    let v1 = run_json(t.path(), &["term","run","--","sh","-c","echo hello"]);
    assert_eq!(v1["errors"].as_array().map(|a| a.len()).unwrap_or(0), 0, "term run should succeed");
    let v2 = run_json(t.path(), &["term","run","--fs-topic","--","sh","-c","echo hi > ok.txt"]);
    assert_eq!(v2["errors"].as_array().map(|a| a.len()).unwrap_or(0), 0, "term run --fs-topic should succeed");

    // Determine run_id from second run (has the same structure as first)
    let run_id = v2["meta"]["run_id"].as_str().unwrap();

    // 2) Extract a TracePack
    let bag_dir = t.path().join("bag");
    let extract = run_json(t.path(), &["replay","extract","--run-id", run_id, "--out", bag_dir.to_str().unwrap()]);
    assert_eq!(extract["errors"].as_array().map(|a| a.len()).unwrap_or(0), 0, "extract should succeed");

    // Assert manifest fields (tools/env/git and counts.*)
    let man_path = bag_dir.join("manifest.json");
    assert!(man_path.exists(), "manifest.json should exist at {}", man_path.display());
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(&man_path).unwrap()).unwrap();
    // counts (must include events_lines, fileops_lines if present, cline_messages possibly 0)
    assert!(manifest.get("counts").is_some(), "manifest.counts missing");
    assert!(manifest.pointer("/counts/events_lines").is_some(), "counts.events_lines missing");
    // tools/env/git snapshots (best-effort; tolerate nulls but require keys present)
    assert!(manifest.get("tools").is_some(), "manifest.tools missing");
    assert!(manifest.get("env").is_some(), "manifest.env missing");
    assert!(manifest.get("git").is_some(), "manifest.git missing");

    // 3) Verify (Verify++)
    let rep = run_status_json(t.path(), &["replay","verify","--path", bag_dir.to_str().unwrap()]);
    assert_eq!(rep.status, 0, "verify should be ok: {:?}", rep.json);
    let data = &rep.json["data"];
    assert!(data["ok"].as_bool().unwrap(), "verify ok=false: {:?}", data);
    // Verify++ sections present
    assert!(data.get("events").is_some(), "verify.events section missing");
    // fileops section is optional; don't assert presence strictly
    assert!(data.get("counts_match").is_some(), "verify.counts_match missing");
    assert!(data.get("patches_check").is_some(), "verify.patches_check missing");
    assert!(data.get("cline_coverage").is_some(), "verify.cline_coverage missing");

    // 4) Doctor CI with profile v0 (preselected checks/gates)
    let doc = run_status_json(t.path(), &["dev","doctor","--ci","--profile","v0"]);
    assert_eq!(doc.status, 0, "doctor --ci --profile v0 should pass: {:?}", doc.json);
    let checks = doc.json["data"]["checks"].as_array().cloned().unwrap_or_default();
    // Ensure cline shows up (info severity acceptable even if no storage configured)
    let has_cline = checks.iter().any(|c| c["name"].as_str() == Some("cline"));
    assert!(has_cline, "doctor.ci output should include cline check: {:?}", checks);
}
