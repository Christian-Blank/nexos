use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, Write},
    path::Path,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Target {
    pub kind: String, // "event" | "fileop" | "bag"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Annotation {
    pub ts: String,            // created-at
    pub source: String,        // human|agent|rule|workflow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    pub target: Target,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<Value>,
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
}

pub fn list(bag: &Path) -> anyhow::Result<Vec<Value>> {
    let p = bag.join("annotations.ndjson");
    if !p.exists() { return Ok(vec![]); }
    let f = fs::File::open(&p)?;
    let br = std::io::BufReader::new(f);
    let mut out = Vec::new();
    for line in br.lines() {
        let s = match line { Ok(s) if !s.trim().is_empty() => s, _ => continue };
        if let Ok(v) = serde_json::from_str::<Value>(&s) { out.push(v); }
    }
    Ok(out)
}

pub fn add_for_event(
    bag: &Path,
    seq: u64,
    labels: Vec<String>,
    note: Option<String>,
    author: Option<String>,
    source: String,
    confidence: Option<f64>,
    attrs: Option<Value>,
) -> anyhow::Result<Annotation> {
    // Prefer TracePack root events; fallback topics/events.ndjson
    let p1 = bag.join("events.ndjson");
    let p2 = bag.join("topics").join("events.ndjson");
    let ev_path = if p1.exists() { p1 } else { p2 };

    let row = pick_row(ev_path.as_path(), seq)
        .with_context(|| "events seq not found")?;
    let ann = Annotation {
        ts: now_rfc3339(),
        source,
        author,
        labels,
        note,
        confidence,
        target: Target {
            kind: "event".into(),
            seq: Some(seq),
            ts: row.get("ts").and_then(|x| x.as_str()).map(|s| s.to_string()),
            run_id: row.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
            plugin: row.get("plugin").and_then(|x| x.as_str()).map(|s| s.to_string()),
            command: row.get("command").and_then(|x| x.as_str()).map(|s| s.to_string()),
            op: None, path: None,
        },
        attrs,
    };
    append(bag, &ann)?;
    Ok(ann)
}

pub fn add_for_fileop(
    bag: &Path,
    seq: u64,
    labels: Vec<String>,
    note: Option<String>,
    author: Option<String>,
    source: String,
    confidence: Option<f64>,
    attrs: Option<Value>,
) -> anyhow::Result<Annotation> {
    // Prefer bag/fileops/fileops.ndjson; fallback bag/fileops.ndjson
    let p1 = bag.join("fileops").join("fileops.ndjson");
    let p2 = bag.join("fileops.ndjson");
    let fo_path = if p1.exists() { p1 } else { p2 };

    let row = pick_row(fo_path.as_path(), seq)
        .with_context(|| "fileops seq not found")?;
    let ann = Annotation {
        ts: now_rfc3339(),
        source,
        author,
        labels,
        note,
        confidence,
        target: Target {
            kind: "fileop".into(),
            seq: Some(seq),
            ts: row.get("ts").and_then(|x| x.as_str()).map(|s| s.to_string()),
            run_id: row.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()),
            plugin: None, command: None,
            op: row.get("op").and_then(|x| x.as_str()).map(|s| s.to_string()),
            path: row.get("path").and_then(|x| x.as_str()).map(|s| s.to_string()),
        },
        attrs,
    };
    append(bag, &ann)?;
    Ok(ann)
}

pub fn add_for_bag(
    bag: &Path,
    labels: Vec<String>,
    note: Option<String>,
    author: Option<String>,
    source: String,
    confidence: Option<f64>,
    attrs: Option<Value>,
) -> anyhow::Result<Annotation> {
    // Try to read bag manifest for run_id
    let run_id = fs::read_to_string(bag.join("manifest.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("run_id").and_then(|x| x.as_str()).map(|s| s.to_string()));

    let ann = Annotation {
        ts: now_rfc3339(),
        source,
        author,
        labels,
        note,
        confidence,
        target: Target {
            kind: "bag".into(),
            seq: None,
            ts: None,
            run_id,
            plugin: None, command: None, op: None, path: None,
        },
        attrs,
    };
    append(bag, &ann)?;
    Ok(ann)
}

fn pick_row(path: &Path, seq: u64) -> anyhow::Result<Value> {
    let f = fs::File::open(path)?;
    let br = std::io::BufReader::new(f);
    let mut idx = 0u64;
    for line in br.lines() {
        let s = line?;
        if s.trim().is_empty() { continue; }
        if idx == seq {
            let v: Value = serde_json::from_str(&s)?;
            return Ok(v);
        }
        idx += 1;
    }
    anyhow::bail!("seq {} out of bounds for {}", seq, path.display());
}

pub fn append(bag: &Path, ann: &Annotation) -> anyhow::Result<()> {
    let p = bag.join("annotations.ndjson");
    if let Some(parent) = p.parent() { fs::create_dir_all(parent)?; }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&p)?;
    writeln!(f, "{}", serde_json::to_string(ann)?)?;
    Ok(())
}
