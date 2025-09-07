use clap::{Arg, ArgAction, ArgMatches, Command};
use nexoscli_core::plugin::CommandContext;
use nexoscli_core::Result;
use std::fs;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Subtree for `dev log ...`
pub fn subcommand() -> Command {
    Command::new("log")
        .about("Event log utilities")
        .subcommand(
            Command::new("rotate")
                .about("Rotate .nexoscli/log/events.jsonl into timestamped segments with chain manifests")
                .arg(
                    Arg::new("max-bytes")
                        .long("max-bytes")
                        .value_parser(clap::value_parser!(u64))
                        .default_value("5000000"),
                )
                .arg(Arg::new("daily").long("daily").action(ArgAction::SetTrue))
                .arg(Arg::new("force").long("force").action(ArgAction::SetTrue))
                .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue)),
        )
        .subcommand(Command::new("status").about("Show rotation/index status"))
}

/// Router for `dev log` commands
pub fn run(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    match m.subcommand() {
        Some(("status", _m)) => status(ctx),
        Some(("rotate", m)) => rotate(m, ctx),
        _ => Ok(2),
    }
}

/// `dev log status`
/// Reads .nexoscli/log/events.index.json and reports segments/head/updated_at
fn status(ctx: &CommandContext) -> Result<i32> {
    let root = std::env::current_dir()?;
    let log_dir = root.join(".nexoscli").join("log");
    let index_path = log_dir.join("events.index.json");

    let (segments, head, updated_at) = if let Ok(txt) = fs::read_to_string(&index_path) {
        let v: serde_json::Value = serde_json::from_str(&txt).unwrap_or(serde_json::json!({
            "segments": []
        }));
        let segs = v.get("segments").and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0);
        let head = v.get("head").cloned().unwrap_or(serde_json::Value::Null);
        let updated = v.get("updated_at").cloned().unwrap_or(serde_json::Value::Null);
        (segs, head, updated)
    } else {
        (0, serde_json::Value::Null, serde_json::Value::Null)
    };

    let exit = ctx.ok(
        "dev",
        "log.status",
        &serde_json::json!({
            "segments": segments,
            "head": head,
            "updated_at": updated_at
        }),
    );
    Ok(exit)
}

/// `dev log rotate`
/// Moves events.jsonl to a segment file, writes chain manifest and updates index.
fn rotate(m: &ArgMatches, ctx: &CommandContext) -> Result<i32> {
    let max_bytes = *m.get_one::<u64>("max-bytes").unwrap();
    let daily = m.get_flag("daily");
    let force = m.get_flag("force");
    let dry = m.get_flag("dry-run");

    let root = std::env::current_dir()?;
    let log_dir = root.join(".nexoscli").join("log");
    let (ev, seg_dir, _idx_path) = super::events_dir_from_log(&log_dir);
    fs::create_dir_all(&seg_dir)?;

    let size = std::fs::metadata(&ev).map(|m| m.len()).unwrap_or(0);
    let mut reason = String::new();

    // Compute stats once
    let stats = super::compute_event_file_stats(&ev)?;
    // daily due: rotate if file has any lines and its day differs from now's day
    let now_day = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
    let daily_due = daily
        && stats
            .to_ts
            .as_deref()
            .map(|ts| ts.split('T').next() != now_day.split('T').next())
            .unwrap_or(false);

    let should_rotate = if force {
        reason = "force".into();
        true
    } else if size >= max_bytes {
        reason = format!("size>={}", max_bytes);
        true
    } else if daily_due {
        reason = "daily".into();
        true
    } else {
        false
    };

    if !should_rotate {
        let exit = ctx.ok(
            "dev",
            "log.rotate.plan",
            &serde_json::json!({
                "would_rotate": false,
                "size": size,
                "reason": "thresholds not met"
            }),
        );
        return Ok(exit);
    }

    // STAMP from last event ts or now
    let stamp = stats
        .to_ts
        .clone()
        .unwrap_or_else(|| OffsetDateTime::now_utc().format(&Rfc3339).unwrap())
        .replace(":", "-");

    let file_name = format!("events-{}.jsonl", stamp);
    let seg_file = seg_dir.join(&file_name);
    let seg_chain = seg_dir.join(format!("events-{}.chain.json", stamp));
    let rel_file = format!(".nexoscli/log/segments/{}", file_name);

    if dry {
        let exit = ctx.ok(
            "dev",
            "log.rotate.plan",
            &serde_json::json!({
                "would_rotate": true,
                "reason": reason,
                "segment": seg_file.display().to_string(),
                "lines": stats.lines,
                "bytes": stats.bytes,
                "sha256_file": stats.sha256_file
            }),
        );
        return Ok(exit);
    }

    // Move log to segment
    if ev.exists() {
        fs::rename(&ev, &seg_file)?;
    } else {
        fs::write(&seg_file, b"")?;
    }

    // Write segment manifest
    let seg_rec = serde_json::json!({
        "schema": "events_segment_chain.v0.1",
        "file": rel_file,
        "from_ts": stats.from_ts,
        "to_ts": stats.to_ts,
        "lines": stats.lines,
        "bytes": stats.bytes,
        "sha256_file": stats.sha256_file
    });
    fs::write(&seg_chain, serde_json::to_vec_pretty(&seg_rec)?)?;

    // Update index
    let mut idx = super::load_events_index(&log_dir)?;
    let prev_head = idx.head.clone();
    let head = super::chain_next(prev_head.as_deref(), seg_rec["sha256_file"].as_str().unwrap());
    idx.segments.push(super::IndexSeg {
        file: rel_file.clone(),
        from_ts: seg_rec["from_ts"].as_str().map(|s| s.to_string()),
        to_ts: seg_rec["to_ts"].as_str().map(|s| s.to_string()),
        lines: stats.lines,
        bytes: stats.bytes,
        sha256_file: seg_rec["sha256_file"].as_str().unwrap().to_string(),
        prev_head: prev_head.clone(),
        head: head.clone(),
    });
    idx.head = Some(head.clone());
    idx.updated_at = Some(OffsetDateTime::now_utc().format(&Rfc3339).unwrap());
    super::save_events_index(&log_dir, &idx)?;

    // Recreate fresh events.jsonl
    fs::write(&ev, b"")?;

    let exit = ctx.ok(
        "dev",
        "log.rotate",
        &serde_json::json!({
            "rotated": true,
            "reason": reason,
            "segment": seg_file.display().to_string(),
            "chain": seg_chain.display().to_string(),
            "index": log_dir.join("events.index.json").display().to_string(),
            "head": idx.head
        }),
    );
    Ok(exit)
}
