use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn issue_happy_path_json() {
    let tmp = tempdir().unwrap();
    let mut new = Command::cargo_bin("nexoscli").unwrap();
    new.current_dir(tmp.path())
        .args(["issue","new","--title","Test Title","--output","json"])
        .assert()
        .success()
        .stdout(contains("\"ISS-0001\""));

    let mut list = Command::cargo_bin("nexoscli").unwrap();
    list.current_dir(tmp.path())
        .args(["issue","list","--output","json"])
        .assert()
        .success()
        .stdout(contains("\"items\""));
}
