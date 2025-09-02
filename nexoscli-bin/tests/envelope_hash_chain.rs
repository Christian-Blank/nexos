mod helpers;
use helpers::*;
use sha2::{Digest, Sha256};
use serde_json::Value;
use std::{env, fs, path::PathBuf};

fn read_run_lines(tmp: &PathBuf, run_id: &str) -> Vec<String> {
    let p = tmp.join(".nexoscli").join("log").join("runs").join(format!("{}.jsonl", run_id));
    assert!(p.exists(), "per-run file does not exist: {:?}", p);
    fs::read_to_string(p).unwrap()
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{:x}", h.finalize())
}

#[test]
fn event_ledger_chain_hashes_link_and_manifest_written() {
    let tdir = tmp();
    let tmp_path = tdir.path().to_path_buf();

    // Force ledger on and a fixed RUN_ID so multiple invokes share the same per-run file
    env::set_var("NEXOS_LEDGER", "1");
    let run_id = "RUN-TEST-CHAIN-0001";
    env::set_var("NEXOS_RUN_ID", run_id);

    // Emit two events
    run_ok(&tmp_path, &["issue", "new", "--title", "Chain A"]);
    run_ok(&tmp_path, &["issue", "list"]);

    // Read per-run lines and parse first two events
    let lines = read_run_lines(&tmp_path, run_id);
    assert!(lines.len() >= 2, "expected at least two events, got {}", lines.len());

    let v1: Value = serde_json::from_str(&lines[0]).expect("parse ev1");
    let v2: Value = serde_json::from_str(&lines[1]).expect("parse ev2");

    // Basic field presence (v0.3)
    assert_eq!(v1.get("schema").and_then(|x| x.as_str()), Some("event.v0.3"));
    assert_eq!(v2.get("schema").and_then(|x| x.as_str()), Some("event.v0.3"));

    let s1 = v1.get("seq").and_then(|x| x.as_u64()).expect("seq1");
    let s2 = v2.get("seq").and_then(|x| x.as_u64()).expect("seq2");
    assert_eq!(s1, 1, "first seq should be 1");
    assert_eq!(s2, 2, "second seq should be 2");

    let ch1 = v1.get("chain_hash").and_then(|x| x.as_str()).expect("chain_hash1").to_string();
    let prev2 = v2.get("prev").and_then(|x| x.as_str()).expect("prev2").to_string();
    assert_eq!(prev2, ch1, "prev of second event must equal chain_hash of first");

    // Recompute chain_hash for second event: sha256(prev + content_hash2)
    let content_hash2 = v2.get("content_hash").and_then(|x| x.as_str()).expect("content_hash2");
    let recomputed2 = sha256_hex(format!("{}{}", prev2, content_hash2).as_bytes());
    let chain_hash2 = v2.get("chain_hash").and_then(|x| x.as_str()).expect("chain_hash2");
    assert_eq!(chain_hash2, recomputed2, "chain_hash mismatch for second event");

    // Manifest assertions
    let manifest_path = tmp_path
        .join(".nexoscli").join("log").join("runs")
        .join(format!("{}.manifest.json", run_id));
    assert!(manifest_path.exists(), "manifest not found: {:?}", manifest_path);

    let mf: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).expect("parse manifest");
    assert_eq!(mf.get("version").and_then(|x| x.as_str()), Some("0.2"));
    assert_eq!(mf.get("run_id").and_then(|x| x.as_str()), Some(run_id));
    let count = mf.get("count").and_then(|x| x.as_u64()).expect("manifest.count");
    assert!(count >= 2, "manifest count expected >=2, got {}", count);
    assert_eq!(mf.get("chain_ok").and_then(|x| x.as_bool()), Some(true));

    // Global events file should include the same enriched lines
    let global_events = read_events(&tmp_path);
    assert!(global_events.iter().any(|e| e.get("schema").and_then(|x| x.as_str()) == Some("event.v0.3")),
        "expected at least one v0.3 event in global events.jsonl");
}
