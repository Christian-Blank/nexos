use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;

#[test]
fn version_plugins_tools_export() {
    Command::cargo_bin("nexoscli").unwrap()
        .args(["version"])
        .assert()
        .success();

    Command::cargo_bin("nexoscli").unwrap()
        .args(["plugins"])
        .assert()
        .success()
        .stdout(contains("issue"))
        .stdout(contains("dev"))
        .stdout(contains("graph"));

    // tools manifest includes key commands
    Command::cargo_bin("nexoscli").unwrap()
        .args(["tools","export"])
        .assert()
        .success()
        .stdout(contains("\"issue.new\""))
        .stdout(contains("\"dev.init\""))
        .stdout(contains("\"graph.check\""));
}
