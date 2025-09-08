mod helpers;
use helpers::*;
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn dev_docs_catalog_json_and_parity_ok() {
    let t = tmp();

    // Generate catalog JSON into the temp working dir
    let _v = run_json(
        t.path(),
        &["dev", "docs", "commands", "--out-json", "docs/catalog/commands.json"],
    );

    // Basic invariants: file exists and schema tag matches
    let p = t.path().join("docs/catalog/commands.json");
    assert!(p.exists(), "catalog not written at {:?}", p);

    // Validate schema tag and non-empty commands via jq-like inspection in Rust
    let txt = fs::read_to_string(&p).expect("read catalog");
    let v: serde_json::Value = serde_json::from_str(&txt).expect("parse catalog");
    assert_eq!(
        v.get("schema").and_then(|x| x.as_str()),
        Some("commands_catalog.v1"),
        "catalog schema tag mismatch"
    );
    let n = v
        .get("commands")
        .and_then(|x| x.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(n > 0, "expected at least one command in catalog");

    // Parity check should succeed
    let s = run_status_json(
        t.path(),
        &[
            "dev",
            "docs",
            "commands",
            "--out-json",
            "docs/catalog/commands.json",
            "--check-parity",
        ],
    );
    assert_eq!(
        s.status, 0,
        "toolspec/catalog parity failed: {}",
        s.json.to_string()
    );
}

#[test]
fn dev_repo_args_enforce_migrated_ok_for_dev_init() {
    // Run in repo root because enforcement scans plugins/* in the cwd
    let repo_root = PathBuf::from(".");
    let tmp_dir = PathBuf::from("target/test-tmp");
    let _ = fs::create_dir_all(&tmp_dir);

    // Prepare migrated list file with a known migrated module using derive Args + JsonSchema + Ser/De
    let migrated_file = tmp_dir.join("args_enforce_migrated.txt");
    fs::write(&migrated_file, "plugins/dev/src/cmd_init.rs\n").expect("write migrated list");

    // Run args-enforce using the JSON envelope helper and assert contract fields
    let s = run_status_json(
        &repo_root,
        &[
            "dev",
            "repo",
            "args-enforce",
            "--migrated",
            migrated_file.to_str().unwrap(),
        ],
    );

    assert_eq!(s.status, 0, "args-enforce exited non-zero: {}", s.json);
    assert_eq!(
        s.json["data"]["ok"].as_bool(),
        Some(true),
        "expected data.ok=true, got: {}",
        s.json
    );
}

#[test]
fn dev_docs_catalog_idempotent_ignoring_generated_at() {
    let t = tmp();

    // First generation
    let _v1 = run_json(
        t.path(),
        &["dev", "docs", "commands", "--out-json", "docs/catalog/commands.json"],
    );
    let p = t.path().join("docs/catalog/commands.json");
    let txt1 = std::fs::read_to_string(&p).expect("read catalog v1");
    let mut v1: serde_json::Value = serde_json::from_str(&txt1).expect("parse v1");

    // Second generation (same path)
    let _v2 = run_json(
        t.path(),
        &["dev", "docs", "commands", "--out-json", "docs/catalog/commands.json"],
    );
    let txt2 = std::fs::read_to_string(&p).expect("read catalog v2");
    let mut v2: serde_json::Value = serde_json::from_str(&txt2).expect("parse v2");

    // Ignore time-based field and compare deterministically
    if let Some(obj) = v1.as_object_mut() {
        obj.insert("generated_at".into(), serde_json::Value::String("X".into()));
    }
    if let Some(obj) = v2.as_object_mut() {
        obj.insert("generated_at".into(), serde_json::Value::String("X".into()));
    }

    assert_eq!(
        v1, v2,
        "catalog not idempotent after normalizing generated_at:\nfirst={}\nsecond={}",
        serde_json::to_string_pretty(&v1).unwrap(),
        serde_json::to_string_pretty(&v2).unwrap()
    );
}
