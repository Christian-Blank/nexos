mod helpers; use helpers::*;

#[test]
fn envelope_meta_has_duration_ms_and_event_has_dur_ms() {
    let t = tmp();
    let v = run_json(t.path(), &["issue","new","--title","Dur probe"]);
    let dur = v["meta"]["duration_ms"].as_u64().expect("duration_ms");
    assert!(dur >= 0);

    let events = helpers::read_events(t.path());
    assert!(events.iter().any(|e| e.get("dur_ms").is_some()), "dur_ms not found in events");
}
