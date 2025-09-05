mod helpers;
use helpers::*;
use std::{fs, path::PathBuf};

#[test]
fn replay_fileops_derive_from_patches_ndjson_and_validate() {
    let t = tmp();

    // Synthesize a "bag-like" directory with patches under .nexoscli/patches
    let patches_dir = t.path().join(".nexoscli").join("patches");
    fs::create_dir_all(&patches_dir).expect("mkdir -p .nexoscli/patches");

    // Create a unified diff with multiple sections: rename, create, delete, modify
    let patch_body = r#"diff --git a/a.txt b/b.txt
similarity index 100%
rename from a.txt
rename to b.txt
--- a/a.txt
+++ b/b.txt
@@ -0,0 +1 @@
+renamed
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
diff --git a/old.txt b/old.txt
deleted file mode 100644
index e69de29..0000000
--- a/old.txt
+++ /dev/null
@@ -1 +0,0 @@
-removed
diff --git a/modified.txt b/modified.txt
index e69de29..abc1234 100644
--- a/modified.txt
+++ b/modified.txt
@@ -0,0 +1 @@
+line
"#;
    let patch_path = patches_dir.join("sample.patch");
    fs::write(&patch_path, patch_body.as_bytes()).expect("write sample.patch");

    // Derive fileops into an output directory
    let bag_dir = t.path().join("bag");
    let out_path: PathBuf = bag_dir.join("fileops.ndjson");
    let v = run_json(
        t.path(),
        &[
            "replay",
            "fileops",
            "derive",
            "--bag",
            ".nexoscli",
            "--out",
            out_path.to_str().unwrap(),
        ],
    );
    let lines = v["data"]["lines"].as_u64().unwrap_or(0);
    assert!(lines >= 1, "expected >=1 fileops lines, got {}", lines);

    // File was written
    assert!(
        out_path.exists(),
        "expected output fileops at {}",
        out_path.display()
    );

    // Validate the derived fileops only (no events in this synthetic bag)
    fs::create_dir_all(&bag_dir).ok();
    // replay validate resolves fileops at bag/fileops.ndjson or bag/fileops/fileops.ndjson
    let v2 = run_json(
        t.path(),
        &[
            "replay",
            "validate",
            "--bag",
            bag_dir.to_str().unwrap(),
            "--what",
            "fileops",
        ],
    );
    let ok = v2["data"]["summary_ok"].as_bool().unwrap_or(false);
    assert!(ok, "expected validation to succeed: {}", v2);
}
