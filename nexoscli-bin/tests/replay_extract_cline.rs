mod helpers; use helpers::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn replay_extract_includes_cline_tasks() {
    let t = tmp();

    // 1) make a run (generates events)
    Command::new("git").current_dir(t.path()).args(["init","-q"]).output().unwrap();
    fs::write(t.path().join("a.txt"), "one\n").unwrap();
    Command::new("git").current_dir(t.path()).args(["add","."]).output().unwrap();
    Command::new("git").current_dir(t.path()).args(["commit","-m","init","-q"]).output().unwrap();

    let j = run_json(t.path(), &["term","run","echo","hello"]);
    assert_eq!(j["errors"], serde_json::Value::Array(vec![]));
    let run_id = j["meta"]["run_id"].as_str().unwrap().to_string();

    // 2) synthesize a normalized cline task in the default location
    let tasks_dir = t.path().join(".nexoscli").join("cline").join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();
    let task = serde_json::json!({
        "id": "task-123",
        "createdAt": "2025-09-01T12:00:00Z",
        "messages": [ {"role":"user","content":"do x"}, {"role":"assistant","content":"ok"} ]
    });
    fs::write(tasks_dir.join("task-123.json"), serde_json::to_vec_pretty(&task).unwrap()).unwrap();

    // 3) extract with cline included (dir)
    let outdir = t.path().join("packs").join("with-cline");
    let v = run_json(t.path(), &["replay","extract","--run-id",&run_id,"--out",outdir.to_str().unwrap(),"--include","cline"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    assert!(outdir.join("manifest.json").exists());

    // verify cline files present
    let cline_dir = outdir.join("cline").join("tasks");
    assert!(cline_dir.exists());
    let mut found = false;
    for ent in fs::read_dir(&cline_dir).unwrap() {
        let p = ent.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) == Some("json") {
            let body = fs::read_to_string(&p).unwrap();
            if body.contains("\"task-123\"") { found = true; }
        }
    }
    assert!(found, "expected cline task copied into tracepack");

    // manifest contains cline count
    let m: serde_json::Value = serde_json::from_str(&fs::read_to_string(outdir.join("manifest.json")).unwrap()).unwrap();
    assert!(m["counts"]["cline"].as_u64().unwrap_or(0) >= 1);
    assert!(m["cline_tasks"].as_array().unwrap().len() >= 1);
}
