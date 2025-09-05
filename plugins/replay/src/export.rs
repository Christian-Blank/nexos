use anyhow::Context;
use csv::Writer;
use serde_json::Value;
use crate::annotate::Annotation;
use std::fs::File;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::collections::{BTreeSet, HashMap};

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

// ---- PLAN-37: annotations export and joins ----

fn load_annotations(bag: &Path) -> anyhow::Result<Vec<Annotation>> {
    let p = bag.join("annotations.ndjson");
    if !p.exists() {
        return Ok(vec![]);
    }
    let f = std::fs::File::open(&p)?;
    let br = std::io::BufReader::new(f);
    let mut out = Vec::new();
    for line in br.lines() {
        let s = match line {
            Ok(s) if !s.trim().is_empty() => s,
            _ => continue,
        };
        if let Ok(a) = serde_json::from_str::<Annotation>(&s) {
            out.push(a);
        }
    }
    Ok(out)
}

fn aggregate_annotations(list: &[&Annotation]) -> (String, String, String, String, String, usize) {
    let mut labels = BTreeSet::<String>::new();
    let mut notes: Vec<String> = vec![];
    let mut authors = BTreeSet::<String>::new();
    let mut sources = BTreeSet::<String>::new();
    let mut sum = 0.0_f64;
    let mut cnt = 0usize;

    for a in list {
        for l in &a.labels {
            labels.insert(l.clone());
        }
        if let Some(n) = &a.note {
            if !n.is_empty() {
                notes.push(n.clone());
            }
        }
        if let Some(x) = &a.author {
            if !x.is_empty() {
                authors.insert(x.clone());
            }
        }
        if !a.source.is_empty() {
            sources.insert(a.source.clone());
        }
        if let Some(c) = a.confidence {
            sum += c;
            cnt += 1;
        }
    }
    let conf = if cnt > 0 {
        format!("{:.3}", sum / (cnt as f64))
    } else {
        "".to_string()
    };
    (
        labels.into_iter().collect::<Vec<_>>().join("|"),
        notes.join(" | "),
        authors.into_iter().collect::<Vec<_>>().join("|"),
        sources.into_iter().collect::<Vec<_>>().join("|"),
        conf,
        list.len(),
    )
}

pub fn export_annotations_csv(bag: &Path, out_path: &Path) -> anyhow::Result<usize> {
    let anns = load_annotations(bag)?;
    ensure_parent_dir(out_path)?;
    let mut w = Writer::from_path(out_path)
        .with_context(|| format!("create {}", out_path.display()))?;
    w.write_record(&[
        "ann_ts",
        "source",
        "author",
        "labels",
        "note",
        "confidence",
        "target_kind",
        "target_seq",
        "target_ts",
        "target_run_id",
        "target_plugin",
        "target_command",
        "target_op",
        "target_path",
        "attrs_json",
    ])?;
    let mut n = 0usize;
    for a in &anns {
        let t = &a.target;
        w.write_record(&[
            a.ts.as_str(),
            a.source.as_str(),
            a.author.as_deref().unwrap_or(""),
            &a.labels.join("|"),
            a.note.as_deref().unwrap_or(""),
            &a.confidence.map(|x| format!("{:.3}", x)).unwrap_or_default(),
            t.kind.as_str(),
            &t.seq.map(|x| x.to_string()).unwrap_or_default(),
            t.ts.as_deref().unwrap_or(""),
            t.run_id.as_deref().unwrap_or(""),
            t.plugin.as_deref().unwrap_or(""),
            t.command.as_deref().unwrap_or(""),
            t.op.as_deref().unwrap_or(""),
            t.path.as_deref().unwrap_or(""),
            &a.attrs.as_ref().and_then(|v| serde_json::to_string(v).ok()).unwrap_or_default(),
        ])?;
        n += 1;
    }
    w.flush()?;
    Ok(n)
}

pub fn join_events_with_annotations(bag: &Path, out_dir: &Path) -> anyhow::Result<PathBuf> {
    let base = out_dir.join("events.csv");
    if !base.exists() {
        export_events_csv(bag, &base)?;
    }
    let anns = load_annotations(bag)?;
    if anns.is_empty() {
        return Ok(base);
    }

    let mut map: HashMap<u64, Vec<&Annotation>> = HashMap::new();
    for a in &anns {
        if a.target.kind == "event" {
            if let Some(seq) = a.target.seq {
                map.entry(seq).or_default().push(a);
            }
        }
    }

    let reader = std::fs::File::open(&base)?;
    let mut r = csv::Reader::from_reader(reader);
    let dst = out_dir.join("events_annotated.csv");
    let mut w = csv::Writer::from_path(&dst)?;

    let mut hdr: Vec<String> = r.headers()?.iter().map(|s| s.to_string()).collect();
    hdr.extend([
        "seq",
        "ann_labels",
        "ann_note",
        "ann_author",
        "ann_source",
        "ann_confidence",
        "ann_count",
    ].map(|s| s.to_string()));
    w.write_record(&hdr)?;

    for (idx, rec) in r.records().enumerate() {
        let rec = rec?;
        let mut row = rec.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let seq = idx as u64;
        if let Some(list) = map.get(&seq) {
            let (labels, note, author, source, conf, count) = aggregate_annotations(list);
            row.extend([
                seq.to_string(),
                labels,
                note,
                author,
                source,
                conf,
                count.to_string(),
            ]);
        } else {
            row.extend([
                seq.to_string(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "0".into(),
            ]);
        }
        w.write_record(&row)?;
    }
    w.flush()?;
    Ok(dst)
}

pub fn join_fileops_with_annotations(bag: &Path, out_dir: &Path) -> anyhow::Result<PathBuf> {
    let base = out_dir.join("fileops.csv");
    if !base.exists() {
        export_fileops_csv(bag, &base)?;
    }
    let anns = load_annotations(bag)?;
    if anns.is_empty() {
        return Ok(base);
    }

    let mut map: HashMap<u64, Vec<&Annotation>> = HashMap::new();
    for a in &anns {
        if a.target.kind == "fileop" {
            if let Some(seq) = a.target.seq {
                map.entry(seq).or_default().push(a);
            }
        }
    }

    let reader = std::fs::File::open(&base)?;
    let mut r = csv::Reader::from_reader(reader);
    let dst = out_dir.join("fileops_annotated.csv");
    let mut w = csv::Writer::from_path(&dst)?;

    let mut hdr: Vec<String> = r.headers()?.iter().map(|s| s.to_string()).collect();
    hdr.extend([
        "seq",
        "ann_labels",
        "ann_note",
        "ann_author",
        "ann_source",
        "ann_confidence",
        "ann_count",
    ].map(|s| s.to_string()));
    w.write_record(&hdr)?;

    for (idx, rec) in r.records().enumerate() {
        let rec = rec?;
        let mut row = rec.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let seq = idx as u64;
        if let Some(list) = map.get(&seq) {
            let (labels, note, author, source, conf, count) = aggregate_annotations(list);
            row.extend([
                seq.to_string(),
                labels,
                note,
                author,
                source,
                conf,
                count.to_string(),
            ]);
        } else {
            row.extend([
                seq.to_string(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "0".into(),
            ]);
        }
        w.write_record(&row)?;
    }
    w.flush()?;
    Ok(dst)
}

/// Extended export supporting annotations and joins (non-breaking: keep existing export_csv for callers that don't need join)
pub fn export_csv_join(
    bag: &Path,
    what: &str,
    out: Option<&Path>,
    join: Option<&str>,
) -> anyhow::Result<(Vec<PathBuf>, ExportCounts)> {
    if !bag.is_dir() {
        return Err(anyhow::anyhow!(
            "bag path is not a directory (archives not yet supported for export v0)"
        ));
    }
    let out_root = out
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| bag.join("exports").join("csv"));
    std::fs::create_dir_all(&out_root)
        .with_context(|| format!("mkdir -p {}", out_root.display()))?;

    let mut written: Vec<PathBuf> = Vec::new();
    let mut counts = ExportCounts::default();

    let want_events = what == "events" || what == "all";
    let want_fileops = what == "fileops" || what == "all";
    let want_annotations = what == "annotations" || what == "all";

    if want_events {
        let outp = out_root.join("events.csv");
        let rows = export_events_csv(bag, &outp)?;
        if rows == 0 {
            // Missing input
            let src1 = bag.join("events.ndjson");
            let src2 = bag.join("topics").join("events.ndjson");
            if !src1.exists() && !src2.exists() {
                counts.skipped += 1;
            }
        }
        counts.events_rows = rows;
        written.push(outp);
    }

    if want_fileops {
        let outp = out_root.join("fileops.csv");
        let rows = export_fileops_csv(bag, &outp)?;
        if rows == 0 {
            let src1 = bag.join("fileops").join("fileops.ndjson");
            let src2 = bag.join("fileops.ndjson");
            if !src1.exists() && !src2.exists() {
                counts.skipped += 1;
            }
        }
        counts.fileops_rows = rows;
        written.push(outp);
    }

    if want_annotations {
        let outp = out_root.join("annotations.csv");
        let _n = export_annotations_csv(bag, &outp)?;
        written.push(outp);
    }

    if matches!(join, Some("annotations")) {
        if want_events || what == "all" {
            let p = join_events_with_annotations(bag, &out_root)?;
            // If annotations were empty, p == base; still list the annotated path for determinism
            if p.ends_with("events_annotated.csv") {
                written.push(p);
            } else {
                // annotations empty → no annotated file created, don't push duplicate
            }
        }
        if want_fileops || what == "all" {
            let p = join_fileops_with_annotations(bag, &out_root)?;
            if p.ends_with("fileops_annotated.csv") {
                written.push(p);
            }
        }
    }

    Ok((written, counts))
}
