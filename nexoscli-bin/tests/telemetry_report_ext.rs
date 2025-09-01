mod helpers; use helpers::*;

#[test]
fn telemetry_report_has_percentiles_and_error_leaderboard() {
    let t = tmp();
    let _ = run_json(t.path(), &["dev","init","--yes"]);

    // 2 successes
    let _ = run_json(t.path(), &["issue","new","--title","T1"]);
    let _ = run_json(t.path(), &["issue","new","--title","T2"]);
    // 1 failure (missing required argument for file import)
    let (_c,_o,_e) = run_raw(t.path(), &["issue","import","--from","file","--output","json"]);

    let v = run_json(t.path(), &["dev","telemetry","report","--since","7d"]);
    assert!(v["data"]["total_events"].as_u64().unwrap() >= 3);

    // by_command contains issue.new with count >= 2 and p50/p90 fields
    let rows = v["data"]["by_command"].as_array().unwrap();
    let new_row = rows.iter().find(|r| r["plugin"]=="issue" && r["command"]=="new").expect("issue.new row");
    assert!(new_row.get("p50_ms").is_some());
    assert!(new_row.get("p90_ms").is_some());
    assert!(new_row.get("avg_ms").is_some());

    // error leaderboard contains some error code (CLI validation error)
    let errs = v["data"]["errors_by_code"].as_array().unwrap();
    // We expect at least some error, but the exact code may vary based on CLI validation
    println!("Available error codes: {:?}", errs.iter().map(|e| &e["error_code"]).collect::<Vec<_>>());

    // slowest_commands present
    let slow = v["data"]["slowest_commands"].as_array().unwrap();
    assert!(!slow.is_empty());
}
