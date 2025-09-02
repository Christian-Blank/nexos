use assert_cmd::prelude::*;
use serde_json::Value;
use std::{env, fs, path::{Path, PathBuf}, process::Command};
use tempfile::TempDir;

pub fn tmp() -> TempDir {
    tempfile::tempdir().expect("tempdir")
}

pub fn run_json(tmp: &Path, args: &[&str]) -> Value {
    // Always request JSON unless user specified one already
    let mut a = args.to_vec();
    if !a.iter().any(|x| *x == "--output") {
        a.push("--output"); a.push("json");
    }
    let out = Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp)
        .args(&a)
        .output()
        .expect("run");
    assert!(out.status.success(), "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        a,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    serde_json::from_slice(&out.stdout).expect("parse json")
}

pub fn run_ok(tmp: &Path, args: &[&str]) {
    let out = Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp)
        .args(args)
        .output()
        .expect("run");
    assert!(out.status.success(), "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

pub fn run_raw(tmp: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp)
        .args(args)
        .output()
        .expect("run");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

pub fn events_path(tmp: &Path) -> PathBuf {
    tmp.join(".nexoscli").join("log").join("events.jsonl")
}

pub fn read_events(tmp: &Path) -> Vec<Value> {
    let p = events_path(tmp);
    if !p.exists() { return vec![]; }
    fs::read_to_string(p).unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

pub fn assert_event_for_run(tmp: &Path, run_id: &str, plugin: &str, command: &str) {
    let evs = read_events(tmp);
    assert!(evs.iter().any(|ev|
        ev.get("run_id").and_then(|v| v.as_str()) == Some(run_id) &&
        ev.get("plugin").and_then(|v| v.as_str()) == Some(plugin) &&
        ev.get("command").and_then(|v| v.as_str()) == Some(command) &&
        ev.get("content_hash").is_some()
    ), "expected event with run_id={}, plugin={}, command={} and content_hash", run_id, plugin, command);
}

/// If NEXOS_TEST_EXPORT_LOGS=1, copy the events file to target/test-logs/<name>.jsonl
pub fn export_events(tmp: &Path, name: &str) {
    if env::var("NEXOS_TEST_EXPORT_LOGS").ok().as_deref() != Some("1") { return; }
    let src = events_path(tmp);
    if !src.exists() { return; }
    let dst_dir = PathBuf::from("target").join("test-logs");
    fs::create_dir_all(&dst_dir).ok();
    let dst = dst_dir.join(format!("{}.jsonl", name));
    let _ = fs::copy(src, dst);
}
