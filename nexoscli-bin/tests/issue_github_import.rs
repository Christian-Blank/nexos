mod helpers; 
use helpers::*;

#[test]
fn github_import_from_fixture_dry_run_and_apply() {
    let t = tmp();

    // Seed a fake GitHub payload
    let fixture = r#"
    [
      {
        "number": 101,
        "title": "Fixture One",
        "body": "body one",
        "state": "open",
        "html_url": "https://example/1",
        "created_at": "2024-12-01T00:00:00Z",
        "updated_at": "2024-12-02T00:00:00Z",
        "labels": [{"name":"bug"},{"name":"p0"}]
      },
      {
        "number": 102,
        "title": "Fixture Two",
        "state": "closed",
        "html_url": "https://example/2",
        "created_at": "2024-12-03T00:00:00Z",
        "updated_at": "2024-12-04T00:00:00Z",
        "labels": []
      }
    ]
    "#;
    std::fs::create_dir_all(t.path().join("tests")).unwrap();
    std::fs::write(t.path().join("tests/gh.json"), fixture).unwrap();

    // dry-run
    let v = run_json(t.path(), &[
        "issue","import","--from","github",
        "--repo","owner/repo",
        "--fixture","tests/gh.json",
        "--dry-run"
    ]);
    assert!(v["data"]["created"].as_array().unwrap().len() == 2);

    // apply
    let v2 = run_json(t.path(), &[
        "issue","import","--from","github",
        "--repo","owner/repo",
        "--fixture","tests/gh.json"
    ]);
    let created = v2["data"]["created"].as_array().unwrap();
    assert_eq!(created.len(), 2);
    // file names exist
    let p1 = t.path().join(".nexoscli/issues").join("ISS-GH_owner_repo_101.json");
    let p2 = t.path().join(".nexoscli/issues").join("ISS-GH_owner_repo_102.json");
    assert!(p1.exists() && p2.exists());

    export_events(t.path(), "issue_github_import");
}
