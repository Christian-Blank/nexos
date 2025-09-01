use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn dev_init_plan_and_apply() {
    let tmp = tempdir().unwrap();

    // Plan
    Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp.path())
        .args(["dev","init","--dry-run","--output","json"])
        .assert()
        .success()
        .stdout(contains("\"planned_actions\""));

    // Apply
    Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp.path())
        .args(["dev","init","--yes","--output","json"])
        .assert()
        .success()
        .stdout(contains("\"applied\": true"));

    // Idempotent
    Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp.path())
        .args(["dev","init","--dry-run","--output","json"])
        .assert()
        .success()
        .stdout(contains("\"planned_actions\": []"));
}
