use assert_cmd::prelude::*;
use predicates::str::contains;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn import_from_file_json() {
    let tmp = tempdir().unwrap();

    // seed file
    let data = r#"
    [
      {"title":"One","labels":["a","b"]},
      {"title":"Two","body":"body text","labels":"x,y"}
    ]
    "#;
    std::fs::write(tmp.path().join("issues.json"), data).unwrap();

    // import
    Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp.path())
        .args(["issue","import","--from","file","--path","issues.json","--output","json"])
        .assert()
        .success()
        .stdout(contains("\"created\""));

    // list shows 2
    Command::cargo_bin("nexoscli").unwrap()
        .current_dir(tmp.path())
        .args(["issue","list","--output","json"])
        .assert()
        .success()
        .stdout(contains("ISS-0001"))
        .stdout(contains("ISS-0002"));
}
