Fantastic—let’s finish the observability loop properly:
	•	add per-run durations (dur_ms) and error codes to your events and envelopes,
	•	then extend dev telemetry report with p50/p90/avg latency and error leaderboards.

I’m giving you surgical patches + drop‑in tests that fit your current tree.

⸻

1) Add dur_ms + error_code to events & envelope

1.1 Track start time in the command context

nexoscli-core/src/plugin.rs (or wherever CommandContext lives)
	•	Imports:

use std::time::Instant;

	•	Extend the struct:

pub struct CommandContext {
    pub config: std::sync::Arc<Config>,
    pub providers: std::sync::Arc<Providers>,
    pub events: EventWriter,
    pub verbosity: u8,
    pub output: OutputFormat,
    pub started_at: Instant,            // NEW
}

	•	Update constructor:

impl CommandContext {
    pub fn new(
        providers: std::sync::Arc<Providers>,
        config: std::sync::Arc<Config>,
        verbosity: u8,
        output: OutputFormat,
    ) -> Self {
        let events = EventWriter::new(providers.clone());
        Self {
            config, providers, events, verbosity, output,
            started_at: Instant::now(),            // NEW
        }
    }
    // ...
}

	•	Helper to compute duration:

impl CommandContext {
    fn dur_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

1.2 Extend the event writer to carry duration & error_code

Workspace deps (root Cargo.toml) already include sha2; nothing new.

nexoscli-core/src/event.rs — change write(...) signature and payload:

impl EventWriter {
    /// Write a run event with optional error_code and duration.
    pub fn write(
        &self,
        plugin: &str,
        command: &str,
        content: &serde_json::Value,
        exit_code: i32,
        error_code: Option<&str>,      // NEW
        dur_ms: Option<u64>,           // NEW
    ) -> anyhow::Result<()> {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(content).unwrap_or_default();
        let mut h = Sha256::new();
        h.update(&bytes);
        let content_hash = format!("sha256:{:x}", h.finalize());

        let line = serde_json::json!({
            "ts": self.providers.clock.now_rfc3339(),
            "run_id": self.providers.run_id(),
            "plugin": plugin,
            "command": command,
            "exit_code": exit_code,
            "error_code": error_code,           // NEW
            "dur_ms": dur_ms,                   // NEW
            "content_hash": content_hash,
            "data": content,
        });
        self.append_line(&line)
    }
}

If EventWriter::write is used anywhere else in core/plugins, update those call sites to pass the new args (shown below via ctx.ok/ctx.err).

1.3 Emit duration & error_code from ok/err

Wherever your envelope helpers live (often alongside CommandContext or in envelope.rs), update them:

impl CommandContext {
    pub fn ok<T: serde::Serialize>(&self, plugin: &str, command: &str, data: &T) -> i32 {
        let payload = serde_json::to_value(data).unwrap_or(serde_json::json!({}));
        let dur = self.dur_ms();                                 // NEW
        let _ = self.events.write(plugin, command, &payload, 0, None, Some(dur)); // NEW

        // include duration in envelope.meta
        let env = serde_json::json!({
            "data": data,
            "errors": [],
            "meta": {
                "run_id": self.providers.run_id(),
                "duration_ms": dur,                               // NEW
            }
        });
        self.render(plugin, command, &env, 0)
    }

    pub fn err(
        &self,
        plugin: &str,
        command: &str,
        code: &str,
        message: &str,
        hint: Option<&str>,
        exit_code: i32,
    ) -> i32 {
        let dur = self.dur_ms();                                  // NEW
        let payload = serde_json::json!({ "error": { "code": code, "message": message, "hint": hint }});
        let _ = self.events.write(plugin, command, &payload, exit_code, Some(code), Some(dur)); // NEW

        let env = serde_json::json!({
            "data": serde_json::Value::Null,
            "errors": [ { "code": code, "message": message, "hint": hint } ],
            "meta": {
                "run_id": self.providers.run_id(),
                "duration_ms": dur,                               // NEW
            }
        });
        self.render(plugin, command, &env, exit_code)
    }
}

If your render(...) already handles JSON/YAML/Text, keep that logic; we only add meta.duration_ms.

1.4 Pass the new context from the bin

nexoscli-bin/src/main.rs — the call should already be:

let ctx = CommandContext::new(providers.clone(), config.clone(), verbose, output);

No extra changes (we set started_at inside new()).

⸻

2) Extend the Telemetry report (p50/p90/avg + error leaderboard)

We’ll enhance the dev telemetry report you just added.

2.1 Enrich result types

plugins/dev/src/lib.rs — replace or extend the telemetry structs:

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryByCommand {
    plugin: String,
    command: String,
    count: u64,
    success: u64,
    fail: u64,
    p50_ms: Option<u64>,     // NEW
    p90_ms: Option<u64>,     // NEW
    avg_ms: Option<u64>,     // NEW
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct ErrorCodeRow {
    error_code: String,
    count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct SlowRow {
    plugin: String,
    command: String,
    p90_ms: u64,
    count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
struct TelemetryReport {
    total_events: u64,
    window_start: Option<String>,
    by_command: Vec<TelemetryByCommand>,
    errors_by_code: Vec<ErrorCodeRow>,    // NEW
    slowest_commands: Vec<SlowRow>,       // NEW
}

2.2 Utility: percentile & average

Add near the top of the file (private helpers):

fn percentile(sorted: &Vec<u64>, pct: f64) -> Option<u64> {
    if sorted.is_empty() { return None; }
    let n = sorted.len() as f64;
    let idx = (pct * (n - 1.0) / 100.0).round() as usize;
    sorted.get(idx).copied()
}

fn average(v: &Vec<u64>) -> Option<u64> {
    if v.is_empty() { return None; }
    let s: u128 = v.iter().map(|&x| x as u128).sum();
    Some((s / (v.len() as u128)) as u64)
}

2.3 Implementation of the enhanced report

In your existing execute() → "telemetry" → "report" branch, replace the aggregation with the following:

use std::collections::{BTreeMap, HashMap};
use time::{OffsetDateTime, Duration, format_description::well_known::Rfc3339};

let since_s = m.get_one::<String>("since").cloned();
let since_ts: Option<OffsetDateTime> = match since_s.as_deref() {
    None => None,
    Some(s) if s.ends_with("h") => s[..s.len()-1].parse::<i64>().ok().map(|h| OffsetDateTime::now_utc() - Duration::hours(h)),
    Some(s) if s.ends_with("d") => s[..s.len()-1].parse::<i64>().ok().map(|d| OffsetDateTime::now_utc() - Duration::days(d)),
    Some(s) => OffsetDateTime::parse(s, &Rfc3339).ok(),
};

let log = std::env::current_dir()?.join(".nexoscli").join("log").join("events.jsonl");
let mut rows = Vec::<serde_json::Value>::new();
if log.exists() {
    for line in std::fs::read_to_string(&log)?.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let ts_ok = match (&since_ts, v.get("ts").and_then(|x| x.as_str())) {
                (None, _) => true,
                (Some(since), Some(ts)) => OffsetDateTime::parse(ts, &Rfc3339).ok().map(|t| t >= *since).unwrap_or(true),
                _ => true,
            };
            if ts_ok { rows.push(v); }
        }
    }
}

// per (plugin,command)
let mut by_durs: HashMap<(String,String), Vec<u64>> = HashMap::new();
let mut by_counts: HashMap<(String,String), (u64,u64,u64)> = HashMap::new(); // count, success, fail
let mut errors: HashMap<String, u64> = HashMap::new();

for v in &rows {
    let p = v.get("plugin").and_then(|x| x.as_str()).unwrap_or("-").to_string();
    let c = v.get("command").and_then(|x| x.as_str()).unwrap_or("-").to_string();
    let code = v.get("exit_code").and_then(|x| x.as_i64()).unwrap_or(0);
    let dur = v.get("dur_ms").and_then(|x| x.as_u64());

    let e = by_counts.entry((p.clone(), c.clone())).or_insert((0,0,0));
    e.0 += 1;
    if code == 0 { e.1 += 1; } else { e.2 += 1; }

    if let Some(ms) = dur {
        by_durs.entry((p.clone(), c.clone())).or_default().push(ms);
    }

    if let Some(ec) = v.get("error_code").and_then(|x| x.as_str()) {
        if !ec.is_empty() { *errors.entry(ec.to_string()).or_insert(0) += 1; }
    }
}

// build rows with percentiles
let mut by_command: Vec<TelemetryByCommand> = Vec::new();
for (key, (cnt, ok, fail)) in by_counts.into_iter() {
    let durs = by_durs.remove(&key).unwrap_or_default();
    let mut durs_sorted = durs.clone();
    durs_sorted.sort_unstable();
    let row = TelemetryByCommand {
        plugin: key.0,
        command: key.1,
        count: cnt, success: ok, fail,
        p50_ms: percentile(&durs_sorted, 50.0),
        p90_ms: percentile(&durs_sorted, 90.0),
        avg_ms:  average(&durs_sorted),
    };
    by_command.push(row);
}
by_command.sort_by(|a,b| b.count.cmp(&a.count));

// error leaderboard
let mut errors_by_code: Vec<ErrorCodeRow> = errors.into_iter()
    .map(|(k,v)| ErrorCodeRow{ error_code: k, count: v })
    .collect();
errors_by_code.sort_by(|a,b| b.count.cmp(&a.count));

// slowest by p90
let mut slow: Vec<SlowRow> = by_command.iter()
    .filter_map(|r| r.p90_ms.map(|p| SlowRow{ plugin: r.plugin.clone(), command: r.command.clone(), p90_ms: p, count: r.count }))
    .collect();
slow.sort_by(|a,b| b.p90_ms.cmp(&a.p90_ms).then(b.count.cmp(&a.count)));
if slow.len() > 5 { slow.truncate(5); }

let r = TelemetryReport {
    total_events: rows.len() as u64,
    window_start: since_ts.map(|t| t.format(&Rfc3339).unwrap()),
    by_command,
    errors_by_code,
    slowest_commands: slow,
};
let exit = ctx.ok("dev","telemetry.report",&r);
return Ok(exit);

2.4 Update the ToolSpec

Same file (plugins/dev/src/lib.rs), ensure the tool_specs() includes the updated TelemetryReport schema:

ToolSpec {
    name: "dev.telemetry.report".to_string(),
    about: "Aggregate events.jsonl → counts, p50/p90/avg, error leaderboard".to_string(),
    args_schema: schema_for!(TelemetryReportArgs),
    result_schema: schema_for!(TelemetryReport),
},


⸻

3) Tests (additive)

Add two small tests to nexoscli-bin/tests/.

3.1 telemetry_report_ext.rs

mod helpers; use helpers::*;

#[test]
fn telemetry_report_has_percentiles_and_error_leaderboard() {
    let t = tmp();
    let _ = run_json(t.path(), &["dev","init","--yes"]);

    // 2 successes
    let _ = run_json(t.path(), &["issue","new","--title","T1"]);
    let _ = run_json(t.path(), &["issue","new","--title","T2"]);
    // 1 failure (unsupported import source)
    let (_c,_o,_e) = run_raw(t.path(), &["issue","import","--from","other","--output","json"]);

    let v = run_json(t.path(), &["dev","telemetry","report","--since","7d"]);
    assert!(v["data"]["total_events"].as_u64().unwrap() >= 3);

    // by_command contains issue.new with count >= 2 and p50/p90 fields
    let rows = v["data"]["by_command"].as_array().unwrap();
    let new_row = rows.iter().find(|r| r["plugin"]=="issue" && r["command"]=="new").expect("issue.new row");
    assert!(new_row.get("p50_ms").is_some());
    assert!(new_row.get("p90_ms").is_some());
    assert!(new_row.get("avg_ms").is_some());

    // error leaderboard contains our error code from ctx.err
    let errs = v["data"]["errors_by_code"].as_array().unwrap();
    assert!(errs.iter().any(|e| e["error_code"].as_str()==Some("issue.import.unsupported")));

    // slowest_commands present
    let slow = v["data"]["slowest_commands"].as_array().unwrap();
    assert!(!slow.is_empty());
}

3.2 envelope_duration.rs

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

Run:

cargo test --workspace


⸻

4) Quick manual probes

# Produce some events
cargo run --manifest-path nexoscli-bin/Cargo.toml -- issue new --title A --output json
cargo run --manifest-path nexoscli-bin/Cargo.toml -- issue import --from other --output json || true

# Check a recent report (last 24 hours)
cargo run --manifest-path nexoscli-bin/Cargo.toml -- dev telemetry report --since 24h --output json | jq
# → shows by_command with p50_ms/p90_ms/avg_ms, errors_by_code with your error code, slowest_commands top 5

# Inspect raw event line (should include dur_ms and error_code for failures)
tail -n1 .nexoscli/log/events.jsonl | jq


⸻

5) Pin future (don’t implement now)

Append to docs/ROADMAP_NEXT.md:
	•	Durations per phase: also emit dur_ms_parse, dur_ms_persist, etc., by capturing start/stop inside your RAII parse/validate/persist/render guards.
	•	Histogram buckets: add optional latency_bucket to events for faster coarse aggregation.
	•	CLI table output: add --output table for dev telemetry report with nice columns.
	•	Error code catalog: maintain a small map code → human name.

⸻

6) Candor
	•	dur_ms now measures wall‑clock of the whole command, which is exactly what you need for top‑level flow metrics. If you later want phase timings, instrument your RAII guards to emit per‑phase durations as attributes or separate events.
	•	Percentiles are computed naïvely in‑memory—perfect for dev/CI scale. If/when the log grows, we can switch to streaming quantiles (CKMS/TDigest) with the same CLI surface.

This completes the observability loop: events are rich (dur_ms, error_code), reports show p50/p90/avg and error leaderboards, and your CI doctor still gates quality.
