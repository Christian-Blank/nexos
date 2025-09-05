use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatchMeta {
    pub sha256: String,
    #[serde(default)]
    pub lines: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileOpLine {
    #[serde(default = "schema_tag")]
    pub schema: String, // "nexos/fileops@0.1"
    #[serde(default)]
    pub ts: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub op: String, // "create" | "modify" | "delete" | "rename"
    pub path: String,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub bytes: Option<u64>,
    #[serde(default)]
    pub digest_after: Option<String>,
    #[serde(default)]
    pub source: Option<String>, // "patch"
    #[serde(default)]
    pub context: Option<Value>,
    #[serde(default)]
    pub patch: Option<PatchMeta>,
}

fn schema_tag() -> String {
    "nexos/fileops@0.1".to_string()
}

fn sha256(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b);
    format!("sha256:{:x}", h.finalize())
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// Derive file operations from unified patch bytes (split by "diff --git" headers).
/// - Detect op by header markers:
///   * "new file mode" => create
///   * "deleted file mode" => delete
///   * "rename from"/"rename to" => rename
///   * else => modify
/// - Path selection:
///   * create/modify => use "b/<path>"
///   * delete => use "a/<path>"
///   * rename => from="a/<old>", to="b/<new>", path="b/<new>"
pub fn derive_from_patch_bytes(
    run_id: Option<&str>,
    default_ts: Option<&str>,
    patch_bytes: &[u8],
) -> Vec<FileOpLine> {
    let s = match std::str::from_utf8(patch_bytes) {
        Ok(x) => x,
        Err(_) => return vec![],
    };
    let mut out: Vec<FileOpLine> = Vec::new();
    // Split on "diff --git "
    let mut current: Vec<&str> = Vec::new();
    let mut sections: Vec<Vec<&str>> = Vec::new();

    for line in s.lines() {
        if line.starts_with("diff --git ") {
            if !current.is_empty() {
                sections.push(current.clone());
                current.clear();
            }
        }
        current.push(line);
    }
    if !current.is_empty() {
        sections.push(current);
    }

    for sec in sections {
        let sec_text = sec.join("\n");
        if sec.is_empty() {
            continue;
        }
        // Header line: diff --git a/<pathA> b/<pathB>
        let header = sec[0];
        let mut a_path: Option<String> = None;
        let mut b_path: Option<String> = None;
        {
            // naive parse
            // tokens: ["diff","--git","a/foo","b/bar"]
            let toks: Vec<&str> = header.split_whitespace().collect();
            if toks.len() >= 4 {
                if toks[2].starts_with("a/") {
                    a_path = Some(toks[2][2..].to_string());
                }
                if toks[3].starts_with("b/") {
                    b_path = Some(toks[3][2..].to_string());
                }
            }
        }
        // Markers
        let mut new_file = false;
        let mut deleted_file = false;
        let mut rename_from: Option<String> = None;
        let mut rename_to: Option<String> = None;

        for l in &sec {
            if l.starts_with("new file mode ") {
                new_file = true;
            } else if l.starts_with("deleted file mode ") {
                deleted_file = true;
            } else if l.starts_with("rename from ") {
                rename_from = Some(l.trim_start_matches("rename from ").to_string());
            } else if l.starts_with("rename to ") {
                rename_to = Some(l.trim_start_matches("rename to ").to_string());
            }
        }

        let op;
        let mut path = String::new();
        let mut from = None;
        let mut to = None;

        if rename_from.is_some() && rename_to.is_some() {
            op = "rename".to_string();
            from = rename_from.clone();
            to = rename_to.clone();
            // prefer b_path as destination if available, else rename_to
            path = b_path
                .clone()
                .or(rename_to.clone())
                .unwrap_or_else(|| a_path.clone().unwrap_or_else(|| "".into()));
        } else if new_file {
            op = "create".to_string();
            path = b_path
                .clone()
                .or(a_path.clone())
                .unwrap_or_else(|| "".into());
        } else if deleted_file {
            op = "delete".to_string();
            path = a_path
                .clone()
                .or(b_path.clone())
                .unwrap_or_else(|| "".into());
        } else {
            op = "modify".to_string();
            path = b_path
                .clone()
                .or(a_path.clone())
                .unwrap_or_else(|| "".into());
        }

        // Meta for the section
        let sec_sha = sha256(sec_text.as_bytes());
        let sec_lines = sec.len() as u64;

        let line = FileOpLine {
            schema: schema_tag(),
            ts: Some(default_ts.map(|s| s.to_string()).unwrap_or_else(now_rfc3339)),
            run_id: run_id.map(|s| s.to_string()),
            op,
            path,
            from,
            to,
            bytes: None,
            digest_after: None,
            source: Some("patch".to_string()),
            context: None,
            patch: Some(PatchMeta {
                sha256: sec_sha,
                lines: Some(sec_lines),
            }),
        };
        // Skip if path empty (malformed header)
        if !line.path.is_empty() {
            out.push(line);
        }
    }

    out
}

/// Iterate bag_dir/patches/* and aggregate derived FileOpLine rows.
pub fn derive_for_bag(
    bag_dir: &Path,
    run_id: Option<&str>,
    default_ts: Option<&str>,
) -> anyhow::Result<Vec<FileOpLine>> {
    let patches_dir = bag_dir.join("patches");
    let mut rows: Vec<FileOpLine> = Vec::new();
    if patches_dir.is_dir() {
        for ent in WalkDir::new(&patches_dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .flatten()
        {
            let p = ent.path();
            if p.is_dir() {
                continue;
            }
            // Allow any extension inside patches/ (e.g., .patch or .diff)
            let bytes = fs::read(p).unwrap_or_default();
            if bytes.is_empty() {
                continue;
            }
            let mut r = derive_from_patch_bytes(run_id, default_ts, &bytes);
            rows.append(&mut r);
        }
    }
    Ok(rows)
}

/// Write NDJSON fileops lines to path; returns number of lines written.
pub fn write_fileops_ndjson(path: &Path, rows: &[FileOpLine]) -> anyhow::Result<usize> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut buf = String::new();
    for r in rows {
        let s = serde_json::to_string(r).unwrap_or_else(|_| "{}".to_string());
        buf.push_str(&s);
        buf.push('\n');
    }
    fs::write(path, buf.as_bytes())?;
    Ok(rows.len())
}

/// Helper for CLI: suggest default output path for --bag DIR
pub fn default_cli_out_for_bag(bag_dir: &Path) -> PathBuf {
    bag_dir.join("fileops.ndjson")
}
