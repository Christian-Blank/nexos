use anyhow::Context;
use csv::Writer;
use serde_json::Value;
use std::fs::File;
use std::io::BufRead;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct ExportCounts {
    pub events_rows: u64,
    pub fileops_rows: u64,
    pub skipped: u64,
}

fn ensure_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    Ok(())
}

fn read_lines(path: &Path) -> anyhow::Result<Vec<String>> {
    let f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let rdr = std::io::BufReader::new(f);
    let mut out = Vec::new();
    for line in rdr.lines() {
        let l = line?;
        if l.trim().is_empty() {
            continue;
        }
        out.push(l);
    }
    Ok(out)
}

fn as_str_opt<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(|x| x.as_str())
}
fn as_u64_opt(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|x| x.as_u64())
}
fn nested_str(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k)?;
    }
    cur.as_str().map(|s| s.to_string())
}
fn first_of_nested(v: &Value, paths: &[&[&str]]) -> Option<String> {
    for p in paths {
        if let Some(s) = nested_str(v, p) {
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

pub fn export_events_csv(bag: &Path, out_path: &Path) -> anyhow::Result<u64> {
    // Resolve source: prefer bag/events.ndjson; fallback bag/topics/events.ndjson
    let p1 = bag.join("events.ndjson");
    let p2 = bag.join("topics").join("events.ndjson");
    let src = if p1.exists() { p1 } else { p2 };

    ensure_parent_dir(out_path)?;
    let mut w = Writer::from_path(out_path)
        .with_context(|| format!("create {}", out_path.display()))?;

    // Header (always)
    w.write_record(&[
        "ts",
        "run_id",
        "plugin",
        "command",
        "exit_code",
        "dur_ms",
        "error_code",
        "args_hash",
        "content_hash",
        "task_id",
        "stage",
        "session_id",
        "attrs_cmd",
        "attrs_cwd",
        "attrs_json",
    ])?;

    if !src.exists() {
        // Missing input → just header
        w.flush()?;
        return Ok(0);
    }

    let lines = read_lines(&src)?;
    let mut rows: u64 = 0;
    for raw in lines {
        let v: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue, // skip non-JSON line
        };

        let ts = as_str_opt(&v, "ts").unwrap_or("");
        let run_id = as_str_opt(&v, "run_id").unwrap_or("");
        let plugin = as_str_opt(&v, "plugin").unwrap_or("");
        let command = as_str_opt(&v, "command").unwrap_or("");
        let exit_code = v.get("exit_code").and_then(|x| x.as_i64()).map(|n| n.to_string()).unwrap_or_default();
        let dur_ms = as_u64_opt(&v, "dur_ms").map(|n| n.to_string()).unwrap_or_default();
        let args_hash = as_str_opt(&v, "args_hash").unwrap_or("");
        let content_hash = as_str_opt(&v, "content_hash").unwrap_or("");

        // meta/attrs/data fields (tolerant)
        let error_code = first_of_nested(
            &v,
            &[
                &["meta", "error_code"],
                &["attrs", "error_code"],
                &["data", "error_code"],
            ],
        )
        .unwrap_or_default();

        let task_id = first_of_nested(
            &v,
            &[
                &["attrs", "task_id"],
                &["meta", "task_id"],
                &["data", "task_id"],
            ],
        )
        .unwrap_or_default();

        let stage = first_of_nested(&v, &[&["attrs", "stage"], &["meta", "stage"], &["data", "stage"]])
            .unwrap_or_default();

        let session_id = first_of_nested(
            &v,
            &[
                &["attrs", "session_id"],
                &["meta", "session_id"],
                &["data", "session_id"],
            ],
        )
        .unwrap_or_default();

        let attrs_cmd = nested_str(&v, &["attrs", "cmd"]).unwrap_or_default();
        let attrs_cwd = nested_str(&v, &["attrs", "cwd"]).unwrap_or_default();
        let attrs_json = v
            .get("attrs")
            .and_then(|a| {
                if a.is_null() {
                    None
                } else {
                    serde_json::to_string(a).ok()
                }
            })
            .unwrap_or_default();

        w.write_record(&[
            ts,
            run_id,
            plugin,
            command,
            &exit_code,
            &dur_ms,
            &error_code,
            args_hash,
            content_hash,
            &task_id,
            &stage,
            &session_id,
            &attrs_cmd,
            &attrs_cwd,
            &attrs_json,
        ])?;
        rows += 1;
    }

    w.flush()?;
    Ok(rows)
}

pub fn export_fileops_csv(bag: &Path, out_path: &Path) -> anyhow::Result<u64> {
    // Resolve source: prefer bag/fileops/fileops.ndjson; fallback bag/fileops.ndjson
    let p1 = bag.join("fileops").join("fileops.ndjson");
    let p2 = bag.join("fileops.ndjson");
    let src = if p1.exists() { p1 } else { p2 };

    ensure_parent_dir(out_path)?;
    let mut w = Writer::from_path(out_path)
        .with_context(|| format!("create {}", out_path.display()))?;

    // Header (always)
    w.write_record(&[
        "ts",
        "run_id",
        "op",
        "path",
        "from",
        "to",
        "bytes",
        "digest_after",
        "patch_sha256",
        "patch_lines",
        "source",
    ])?;

    if !src.exists() {
        w.flush()?;
        return Ok(0);
    }

    let lines = read_lines(&src)?;
    let mut rows: u64 = 0;
    for raw in lines {
        let v: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ts = as_str_opt(&v, "ts").unwrap_or("");
        let run_id = as_str_opt(&v, "run_id").unwrap_or("");
        let op = as_str_opt(&v, "op").unwrap_or("");
        let path = as_str_opt(&v, "path").unwrap_or("");
        let from = as_str_opt(&v, "from").unwrap_or("");
        let to = as_str_opt(&v, "to").unwrap_or("");
        let bytes = as_u64_opt(&v, "bytes").map(|n| n.to_string()).unwrap_or_default();
        let digest_after = as_str_opt(&v, "digest_after").unwrap_or("");
        let patch_sha256 = nested_str(&v, &["patch", "sha256"]).unwrap_or_default();
        let patch_lines = v
            .get("patch")
            .and_then(|p| p.get("lines"))
            .and_then(|x| x.as_u64())
            .map(|n| n.to_string())
            .unwrap_or_default();
        let source = as_str_opt(&v, "source").unwrap_or("");

        w.write_record(&[
            ts,
            run_id,
            op,
            path,
            from,
            to,
            &bytes,
            digest_after,
            &patch_sha256,
            &patch_lines,
            source,
        ])?;
        rows += 1;
    }

    w.flush()?;
    Ok(rows)
}

pub fn export_csv(bag: &str, what: &str, out_dir: Option<&str>) -> anyhow::Result<(Vec<String>, ExportCounts)> {
    let bagp = PathBuf::from(bag);
    if !bagp.is_dir() {
        return Err(anyhow::anyhow!("bag path is not a directory (archives not yet supported for export v0)"));
    }
    let out_root = out_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| bagp.join("exports").join("csv"));
    std::fs::create_dir_all(&out_root)
        .with_context(|| format!("mkdir -p {}", out_root.display()))?;

    let mut written: Vec<String> = Vec::new();
    let mut counts = ExportCounts::default();

    let do_events = what == "events" || what == "all";
    let do_fileops = what == "fileops" || what == "all";

    if do_events {
        let outp = out_root.join("events.csv");
        let rows = export_events_csv(&bagp, &outp)?;
        if rows == 0 {
            // No input found; the CSV has only header
            let src1 = bagp.join("events.ndjson");
            let src2 = bagp.join("topics").join("events.ndjson");
            if !src1.exists() && !src2.exists() {
                counts.skipped += 1;
            }
        }
        counts.events_rows = rows;
        written.push(outp.display().to_string());
    }

    if do_fileops {
        let outp = out_root.join("fileops.csv");
        let rows = export_fileops_csv(&bagp, &outp)?;
        if rows == 0 {
            let src1 = bagp.join("fileops").join("fileops.ndjson");
            let src2 = bagp.join("fileops.ndjson");
            if !src1.exists() && !src2.exists() {
                counts.skipped += 1;
            }
        }
        counts.fileops_rows = rows;
        written.push(outp.display().to_string());
    }

    Ok((written, counts))
}
