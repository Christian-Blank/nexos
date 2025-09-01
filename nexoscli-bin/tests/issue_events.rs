mod helpers; use helpers::*;

#[test]
fn envelope_and_events_for_issue_new_close() {
    let t = tmp();

    // new
    let v = run_json(t.path(), &["issue","new","--title","Events please"]);
    // envelope shape
    assert!(v.get("data").is_some());
    assert!(v.get("errors").is_some());
    assert!(v.get("meta").is_some());
    let run1 = v["meta"]["run_id"].as_str().unwrap();
    assert_event_for_run(t.path(), run1, "issue", "new");

    // close
    let id = v["data"]["id"].as_str().unwrap();
    let v2 = run_json(t.path(), &["issue","close", id]);
    let run2 = v2["meta"]["run_id"].as_str().unwrap();
    assert_event_for_run(t.path(), run2, "issue", "close");

    export_events(t.path(), "issue_events");
}
