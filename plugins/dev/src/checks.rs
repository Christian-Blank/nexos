use serde::Serialize;
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};

pub fn redaction_ok() -> bool {
    let p = std::path::Path::new(".nexoscli").join("redact.yaml");
    let s = match std::fs::read_to_string(&p) { Ok(x) => x, Err(_) => return false };
    serde_yaml::from_str::<serde_json::Value>(&s).is_ok()
}

#[derive(Debug, Serialize)]
pub struct EventsWindow {
    pub ok: bool,
    pub total_lines: u64,
    pub with_task: u64,
    #[serde(skip_serializing_if="Option::is_none")]
    pub window_start: Option<String>,
}

pub fn events_window(since: &str) -> anyhow::Result<EventsWindow> {
    // Parse window
    let since_ts: Option<OffsetDateTime> = if since.ends_with('h') {
        since[..since.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h))
    } else if since.ends_with('d') {
        since[..since.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d))
    } else {
        OffsetDateTime::parse(since, &Rfc3339).ok()
    };

    let log = std::path::Path::new(".nexoscli").join("log").join("events.jsonl");
    if !log.exists() {
        return Ok(EventsWindow { ok: true, total_lines: 0, with_task: 0,
            window_start: since_ts.map(|t| t.format(&Rfc3339).unwrap()) });
    }

    let mut total: u64 = 0;
    let mut with_task: u64 = 0;
    let txt = std::fs::read_to_string(&log).unwrap_or_default();
    for line in txt.lines() {
        // Ignore malformed lines
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => { total += 1; continue; }
        };

        // Time filter (tolerant: include if no parseable ts)
        let in_window = match (since_ts, v.get("ts").and_then(|x| x.as_str()).and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())) {
            (None, _)        => true,
            (Some(_), None)  => true,
            (Some(s), Some(t)) => t >= s,
        };
        if !in_window { continue; }

        total += 1;

        // Count if we have context.task_id
        if v.get("context")
            .and_then(|c| c.get("task_id"))
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .is_some()
        {
            with_task += 1;
        }
    }

    Ok(EventsWindow {
        ok: true,
        total_lines: total,
        with_task,
        window_start: since_ts.map(|t| t.format(&Rfc3339).unwrap()),
    })
}
