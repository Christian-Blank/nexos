mod helpers;
use helpers::*;
use serde_json::json;
use std::{fs, path::PathBuf};
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};

#[test]
fn agents_hai_normalize_correlate_and_bag() {
    let t = tmp();

    // Fake Cline storage
    let root = t.path().to_path_buf();
    std::env::set_var("NEXOS_CLINE_TEST_ROOT", root.join("cline-global").display().to_string());
    let tasks_dir = root.join("cline-global").join("saoudrizwan.claude-dev").join("tasks");
    fs::create_dir_all(&tasks_dir).unwrap();

    // Create a task with messages surrounding "now"
    let now = OffsetDateTime::now_utc();
    let u1 = now.format(&Rfc3339).unwrap();
    let a1 = (now + Duration::milliseconds(800)).format(&Rfc3339).unwrap();

    let task = json!({
        "id":"task_norm",
        "title":"demo",
        "createdAt": u1,
        "updatedAt": a1,
        "messages": [
            { "role":"user", "content":"Write a hello world in Rust\n```rust\nfn main() { println!(\"hi\"); }\n```", "createdAt": u1 },
            { "role":"assistant", "content":"Here you go:\n```rust\nfn main(){println!(\"hi\");}\n```", "createdAt": a1 }
        ]
    });
    fs::write(tasks_dir.join("task_norm.json"), serde_json::to_vec_pretty(&task).unwrap()).unwrap();

    // init + import + normalize
    run_json(t.path(), &["dev","init","--yes"]);
    // import explicitly from our fake storage
    run_json(t.path(), &["cline","import","--storage", tasks_dir.to_str().unwrap(), "--since","7d"]);
    run_json(t.path(), &["cline","normalize","--since","7d"]);

    // Check stream exists
    assert!(t.path().join(".nexoscli/agents/streams/Cline/task_norm/messages.ndjson").exists());

    // Create a run near "now"
    run_json(t.path(), &["term","run","--","bash","-lc","echo x > x.txt"]);

    // last run_id from events.jsonl
    let evp = t.path().join(".nexoscli/log/events.jsonl");
    let evs = fs::read_to_string(&evp).unwrap_or_default();
    let mut run_id: Option<String> = None;
    for line in evs.lines().rev() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("plugin").and_then(|x| x.as_str()) == Some("term") {
                if let Some(r) = v.get("run_id").and_then(|x| x.as_str()) {
                    run_id = Some(r.to_string());
                    break;
                }
            }
        }
    }
    let run_id = run_id.expect("run_id");

    // correlate (persist)
    run_json(t.path(), &["cline","correlate","--run-id",&run_id,"--pad","5m","--write"]);
    assert!(t.path().join(".nexoscli/cline/corr").join(format!("{}.json", run_id)).exists());

    // extract bag
    let bag = t.path().join("bag");
    run_json(t.path(), &["replay","extract","--run-id",&run_id,"--include","cline","--out", bag.to_str().unwrap()]);

    // verify messages.ndjson in bag
    let msgs = bag.join("cline/messages.ndjson");
    assert!(msgs.exists());
    let body = fs::read_to_string(&msgs).unwrap();
    assert!(body.contains("\"role\":\"user\""), "expected user message in bag");
    assert!(body.contains("\"role\":\"assistant\""), "expected assistant message in bag");

    // telemetry hai
    let r = run_json(t.path(), &["dev","telemetry","hai","--since","7d"]);
    assert!(r["data"]["prompts"].as_u64().unwrap_or(0) >= 1);
    assert!(r["data"]["responses"].as_u64().unwrap_or(0) >= 1);
}
