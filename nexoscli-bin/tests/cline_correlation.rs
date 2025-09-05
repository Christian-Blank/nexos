mod helpers;
use helpers::*;
use std::{fs, path::PathBuf};
use serde_json::json;
use filetime::{FileTime, set_file_mtime};

#[test]
fn cline_correlation_and_bag_include() {
    let t = tmp();

    // 1) Set up fake Cline storage in test root (via NEXOS_CLINE_TEST_ROOT)
    let root = t.path().to_path_buf();
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", root.join("cline-global").display().to_string());
    let tasks_dir = root.join("cline-global").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();

    // Create a tiny task file with timestamps inferred from fs mtime (fallback)
    let task_path = tasks_dir.join("task_1.json");
    let task = json!({
        "id":"task_1",
        "title":"demo",
        "messages":[{"role":"user","content":"hi"}]
    });
    fs::write(&task_path, serde_json::to_vec_pretty(&task).unwrap()).unwrap();
    // Ensure mtime ~ now so window overlap holds
    set_file_mtime(&task_path, FileTime::now()).unwrap();

    // 2) init and import cline (dry-run not necessary here)
    run_json(t.path(), &["dev","init","--yes"]);
    run_json(t.path(), &["cline","import","--since","7d"]);

    // 3) produce a term.run near "now"
    // Use a simple command (echo) to generate events for a run
    let _ = run_json(t.path(), &["term","run","echo","hi"]);

    // Last run_id from log
    let events = fs::read_to_string(t.path().join(".nexoscli/log/events.jsonl")).unwrap();
    let run_id = events.lines().rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v.get("plugin").and_then(|x| x.as_str())==Some("term"))
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()))
        .expect("run found");

    // 4) correlate & persist
    run_json(t.path(), &["cline","correlate","--run-id",&run_id,"--pad","5m","--write"]);
    assert!(t.path().join(".nexoscli/cline/corr").join(format!("{}.json", run_id)).exists(), "expected persisted correlation record");

    // 5) extract bag with cline included
    let bag = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id",&run_id,"--include","cline","--out", bag.to_str().unwrap()]);
    let cline_tasks = bag.join("cline/tasks");
    assert!(cline_tasks.exists(), "expected cline/tasks directory in bag");

    // At least one cline task should be copied
    let mut any = false;
    if let Ok(rd) = fs::read_dir(&cline_tasks) {
        for ent in rd {
            let p = ent.unwrap().path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                let body = fs::read_to_string(&p).unwrap();
                if body.contains("\"task_1\"") { any = true; break; }
            }
        }
    }
    assert!(any, "expected correlated cline task copied into bag");

    // manifest contains cline list and counts
    let m: serde_json::Value = serde_json::from_slice(&fs::read(bag.join("manifest.json")).unwrap()).unwrap();
    assert!(m["counts"]["cline"].as_u64().unwrap_or(0) >= 1, "expected counts.cline >= 1");
    assert!(m["cline_tasks"].as_array().unwrap().len() >= 1, "expected cline_tasks entries in manifest");
}
