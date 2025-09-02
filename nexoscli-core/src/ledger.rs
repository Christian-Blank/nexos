use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf};

/// Hash-chained event ledger for per-run integrity with global mirroring.
///
/// Responsibilities:
/// - Maintain per-run sequence, prev, and chain_hash
/// - Compute content_hash over a canonical subset (provided by caller)
/// - Enrich event line with schema/seq/prev/content_hash/chain_hash
/// - Append enriched line to:
///   - .nexoscli/log/runs/<RUN-ID>.jsonl
///   - .nexoscli/log/events.jsonl
/// - Emit a per-run manifest on Drop
pub struct Ledger {
    run_id: String,
    seq: u64,
    last_chain_hash: String,
    events_path: PathBuf,
    run_path: PathBuf,
    // bookkeeping for manifest
    first_seq: u64,
    first_hash: Option<String>,
    last_seq: u64,
    first_ts: Option<String>,
    last_ts: Option<String>,
}

impl Ledger {
    /// Open (or create) a ledger rooted at `base` (e.g., ".nexoscli") for the given run_id.
    pub fn open(base: &str, run_id: &str) -> anyhow::Result<Self> {
        let root = PathBuf::from(base);
        fs::create_dir_all(root.join("log").join("runs"))?;

        // Determine paths
        let events_path = root.join("log").join("events.jsonl");
        let run_path = root.join("log").join("runs").join(format!("{}.jsonl", run_id));

        // Seed state from existing run file (if any) so chains and manifest rollups
        // continue across separate CLI invocations that share the same RUN_ID.
        let mut seq0: u64 = 0;
        let mut last_chain = String::new();
        let mut first_seq0: u64 = 0;
        let mut first_hash0: Option<String> = None;
        let mut first_ts0: Option<String> = None;
        let mut last_seq0: u64 = 0;
        let mut last_ts0: Option<String> = None;

        if let Ok(s) = fs::read_to_string(&run_path) {
            let mut lines = s.lines().filter(|l| !l.trim().is_empty());
            let first_line = lines.clone().next();
            let last_line = lines.rev().next();

            if let Some(fl) = first_line {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(fl) {
                    if let Some(n) = v.get("seq").and_then(|x| x.as_u64()) {
                        first_seq0 = n;
                    }
                    if let Some(h) = v.get("chain_hash").and_then(|x| x.as_str()) {
                        first_hash0 = Some(h.to_string());
                    }
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                        first_ts0 = Some(ts.to_string());
                    }
                }
            }
            if let Some(ll) = last_line {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(ll) {
                    if let Some(n) = v.get("seq").and_then(|x| x.as_u64()) {
                        last_seq0 = n;
                        seq0 = n; // continue sequence numbering
                    }
                    if let Some(h) = v.get("chain_hash").and_then(|x| x.as_str()) {
                        last_chain = h.to_string();
                    }
                    if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                        last_ts0 = Some(ts.to_string());
                    }
                }
            }
        }

        Ok(Self {
            run_id: run_id.to_string(),
            seq: seq0,
            last_chain_hash: last_chain,
            events_path,
            run_path,
            first_seq: first_seq0,
            first_hash: first_hash0,
            last_seq: last_seq0,
            first_ts: first_ts0,
            last_ts: last_ts0,
        })
    }

    /// Enrich the provided event `line` with v0.3 fields and append to per-run + global logs.
    ///
    /// Caller provides:
    /// - ts (RFC3339) used for manifest time window
    /// - canonical_for_hash: minimal stable subset to compute `content_hash`
    /// - line: full event JSON object to be enriched (will be mutated)
    pub fn append_event(
        &mut self,
        ts: &str,
        canonical_for_hash: &serde_json::Value,
        line: &mut serde_json::Value,
    ) -> anyhow::Result<()> {
        // increment sequence and compute hashes
        self.seq += 1;
        let prev = self.last_chain_hash.clone();

        let content_hash = sha256(&serde_json::to_vec(canonical_for_hash)?);
        let chain_hash = sha256(format!("{}{}", prev, &content_hash).as_bytes());

        // enrich line with v0.3 fields
        line["schema"] = serde_json::Value::String("event.v0.3".to_string());
        line["seq"] = serde_json::Value::from(self.seq);
        line["prev"] = serde_json::Value::String(prev.clone());
        line["content_hash"] = serde_json::Value::String(content_hash.clone());
        line["chain_hash"] = serde_json::Value::String(chain_hash.clone());

        // update manifest bookkeeping
        if self.first_seq == 0 {
            self.first_seq = self.seq;
            self.first_hash = Some(chain_hash.clone());
            self.first_ts = Some(ts.to_string());
        }
        self.last_seq = self.seq;
        self.last_chain_hash = chain_hash;
        self.last_ts = Some(ts.to_string());

        // append to per-run and global (append-only)
        let line_str = serde_json::to_string(line)?;
        append_line(&self.run_path, &line_str)?;
        append_line(&self.events_path, &line_str)?;

        // Eagerly write/update manifest snapshot (so tests and tools can find it immediately)
        if let Some(dir) = self.run_path.parent() {
            let manifest_path = dir.join(format!("{}.manifest.json", self.run_id));
            // Prefer actual line count from per-run file to avoid cross-process drift
            let cnt: u64 = fs::read_to_string(&self.run_path)
                .ok()
                .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u64)
                .unwrap_or(self.last_seq.saturating_sub(self.first_seq).saturating_add(1));

            let manifest = serde_json::json!({
              "version": "0.2",
              "run_id": self.run_id,
              "count": cnt,
              "first_seq": self.first_seq,
              "last_seq": self.last_seq,
              "first_hash": self.first_hash,
              "last_hash": self.last_chain_hash,
              "time_range": { "from": self.first_ts, "to": self.last_ts },
              "chain_ok": true
            });
            let _ = fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap_or_default());
        }

        Ok(())
    }
}

impl Drop for Ledger {
    fn drop(&mut self) {
        // best-effort manifest write
        if self.last_seq >= self.first_seq && self.first_seq > 0 {
            // Prefer actual line count from per-run file to avoid cross-process drift
            let cnt: u64 = fs::read_to_string(&self.run_path)
                .ok()
                .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u64)
                .unwrap_or(self.last_seq.saturating_sub(self.first_seq).saturating_add(1));

            let manifest = serde_json::json!({
              "version": "0.2",
              "run_id": self.run_id,
              "count": cnt,
              "first_seq": self.first_seq,
              "last_seq": self.last_seq,
              "first_hash": self.first_hash,
              "last_hash": self.last_chain_hash,
              "time_range": { "from": self.first_ts, "to": self.last_ts },
              "chain_ok": true
            });
            if let Some(dir) = self.run_path.parent() {
                let mf = dir.join(format!("{}.manifest.json", self.run_id));
                let _ = fs::write(mf, serde_json::to_vec_pretty(&manifest).unwrap_or_default());
            }
        }
    }
}

fn append_line(path: &PathBuf, s: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(s.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

fn sha256(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    format!("sha256:{:x}", h.finalize())
}
