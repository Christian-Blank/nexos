mod helpers;
use helpers::*;
use std::fs;
use std::process::Command;

#[test]
fn replay_extract_dir_and_tgz() {
    let t = tmp();

    // init git and create a change via term run (captures a patch)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "one\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    let j = run_json(t.path(), &["term","run","sh","-lc","echo two >> a.txt"]);
    let run_id = j["meta"]["run_id"].as_str().unwrap();

    // extract to directory
    let outdir = t.path().join("packs").join("run1");
    let v = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", outdir.to_str().unwrap()]);
    assert!(v["data"]["out"].as_str().unwrap().ends_with("packs/run1"));
    assert!(outdir.join("manifest.json").exists());
    assert!(outdir.join("events.ndjson").exists());
    assert!(outdir.join("README.md").exists());

    // verify manifest structure
    let manifest_text = fs::read_to_string(outdir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    assert_eq!(manifest["version"], "0.1");
    assert_eq!(manifest["kind"], "tracepack");
    assert_eq!(manifest["run_id"], run_id);
    assert!(manifest["counts"].is_object());
    assert!(manifest["created_at"].is_string());

    // extract to tar.gz
    let tgz = t.path().join("packs").join("run1.tgz");
    let v2 = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", tgz.to_str().unwrap()]);
    assert_eq!(v2["data"]["format"], "tgz");
    assert!(t.path().join("packs").join("run1.tgz").exists());

    // verify tarball can be extracted
    let extract_test_dir = t.path().join("extract-test");
    fs::create_dir_all(&extract_test_dir).unwrap();
    let tar_output = Command::new("tar")
        .current_dir(&extract_test_dir)
        .args(["-tzf", tgz.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(tar_output.status.success());
    let contents = String::from_utf8_lossy(&tar_output.stdout);
    assert!(contents.contains("manifest.json"));
    assert!(contents.contains("events.ndjson"));
    assert!(contents.contains("README.md"));
}

#[test]
fn replay_extract_events_only() {
    let t = tmp();

    // Create some activity
    let j1 = run_json(t.path(), &["issue", "new", "--title", "Test issue"]);
    let run_id = j1["meta"]["run_id"].as_str().unwrap();

    // Extract events only
    let outdir = t.path().join("events-only");
    let v = run_json(t.path(), &["replay","extract","--run-id",run_id,"--out", outdir.to_str().unwrap(), "--include", "events"]);
    
    assert!(outdir.join("manifest.json").exists());
    assert!(outdir.join("events.ndjson").exists());
    assert!(outdir.join("README.md").exists());
    assert!(!outdir.join("patches").exists());
    assert!(!outdir.join("status").exists());

    // Verify events file contains our run
    let events_content = fs::read_to_string(outdir.join("events.ndjson")).unwrap();
    assert!(events_content.contains(run_id));
    assert!(events_content.contains("issue"));
    assert!(events_content.contains("new"));
}

#[test]
fn replay_extract_nonexistent_run() {
    let t = tmp();

    let outdir = t.path().join("nonexistent");
    let v = run_json(t.path(), &["replay","extract","--run-id","NONEXISTENT-12345","--out", outdir.to_str().unwrap()]);
    
    // Should still succeed but with zero events
    assert_eq!(v["data"]["counts"]["events_lines"], 0);
    assert!(outdir.join("manifest.json").exists());
    assert!(outdir.join("events.ndjson").exists());
    
    // Events file should be empty
    let events_content = fs::read_to_string(outdir.join("events.ndjson")).unwrap();
    assert!(events_content.is_empty());
}

#[test]
fn replay_extract_manifest_validation() {
    let t = tmp();

    // Create multiple commands to test plugin/command counting
    let j1 = run_json(t.path(), &["issue", "new", "--title", "Test 1"]);
    let run_id1 = j1["meta"]["run_id"].as_str().unwrap();
    
    let j2 = run_json(t.path(), &["issue", "new", "--title", "Test 2"]);
    let run_id2 = j2["meta"]["run_id"].as_str().unwrap();

    // Extract first run
    let outdir = t.path().join("manifest-test");
    run_json(t.path(), &["replay","extract","--run-id",run_id1,"--out", outdir.to_str().unwrap()]);

    // Verify manifest structure and data
    let manifest_text = fs::read_to_string(outdir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).unwrap();
    
    // Check required fields
    assert_eq!(manifest["version"], "0.1");
    assert_eq!(manifest["kind"], "tracepack");
    assert_eq!(manifest["run_id"], run_id1);
    assert!(manifest["created_at"].is_string());
    
    // Check counts structure
    assert!(manifest["counts"]["events_lines"].is_number());
    assert!(manifest["counts"]["steps"].is_number());
    assert!(manifest["counts"]["patches"].is_number());
    assert!(manifest["counts"]["status"].is_number());
    
    // Check git info
    assert!(manifest["git"]["from"].is_string());
    assert!(manifest["git"]["to"].is_string());
    
    // Check plugin/command tallies
    assert!(manifest["plugins"].is_object());
    assert!(manifest["commands"].is_object());
    
    // Should have issue plugin activity
    if manifest["counts"]["events_lines"].as_u64().unwrap() > 0 {
        assert!(manifest["plugins"]["issue"].is_number());
        assert!(manifest["commands"]["issue.new"].is_number());
    }
}

#[test]
fn replay_extract_destination_exists_error() {
    let t = tmp();

    // Create initial extraction
    let j = run_json(t.path(), &["issue", "new", "--title", "Test"]);
    let run_id = j["meta"]["run_id"].as_str().unwrap();
    
    let outdir = t.path().join("existing");
    fs::create_dir_all(&outdir).unwrap();
    fs::write(outdir.join("existing.txt"), "already exists").unwrap();

    // Try to extract to existing directory - should fail
    let (code, _stdout, stderr) = run_raw(t.path(), &["replay","extract","--run-id",run_id,"--out", outdir.to_str().unwrap()]);
    assert_ne!(code, 0);
    assert!(stderr.contains("destination exists") || stderr.contains("Error"));
}

#[test]
fn replay_plan_and_run_basic() {
    let t = tmp();

    // Create some activity
    let j = run_json(t.path(), &["issue", "new", "--title", "Plan test"]);
    let run_id = j["meta"]["run_id"].as_str().unwrap();

    // Test plan subcommand
    let plan_result = run_json(t.path(), &["replay","plan","--run-id",run_id]);
    assert_eq!(plan_result["data"]["run_id"], run_id);
    assert!(plan_result["data"]["count"].is_number());
    assert!(plan_result["data"]["from"].is_string());
    assert!(plan_result["data"]["to"].is_string());

    // Test run subcommand (dry-run)
    let run_result = run_json(t.path(), &["replay","run","--run-id",run_id,"--dry-run"]);
    assert_eq!(run_result["data"]["run_id"], run_id);
    assert_eq!(run_result["data"]["dry_run"], true);
    assert_eq!(run_result["data"]["branch"], "replay");
    assert!(run_result["data"]["applied"].is_number());
}
