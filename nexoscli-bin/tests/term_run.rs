mod helpers;
use helpers::*;

#[test]
fn term_run_captures_output_and_exit_code() {
    let t = tmp();
    let v = run_json(t.path(), &["term","run","echo","hello"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    let data = &v["data"];
    assert_eq!(data["exit_code"], 0);
    assert!(data["stdout_sample"].as_str().unwrap().contains("hello"));
    assert!(data["stdout_sha256"].as_str().unwrap().starts_with("sha256:"));
    assert!(data["stderr_sha256"].as_str().unwrap().starts_with("sha256:"));
}

#[test]
fn term_run_captures_failure_exit_code() {
    let t = tmp();
    // Use run_raw since the command will exit with non-zero but that's expected behavior
    let (exit_code, stdout, _stderr) = run_raw(t.path(), &["--output","json","term","run","false"]);
    assert_ne!(exit_code, 0); // Should match the wrapped command's exit code
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = &v["data"];
    assert_ne!(data["exit_code"], 0);
    assert!(data["stdout_sha256"].as_str().unwrap().starts_with("sha256:"));
    assert!(data["stderr_sha256"].as_str().unwrap().starts_with("sha256:"));
}

#[test]
fn term_run_with_custom_cwd() {
    let t = tmp();
    std::fs::create_dir_all(t.path().join("subdir")).unwrap();
    let subdir = t.path().join("subdir");
    
    // Use run_raw to avoid the --output json flag being passed to pwd
    let (exit_code, stdout, _stderr) = run_raw(t.path(), &["--output","json","term","run","--cwd", subdir.to_str().unwrap(), "pwd"]);
    assert_eq!(exit_code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = &v["data"];
    assert_eq!(data["exit_code"], 0);
    assert_eq!(data["cwd"], subdir.to_str().unwrap());
    
    // On Unix systems, pwd should show the directory path
    #[cfg(unix)]
    {
        let stdout_sample = data["stdout_sample"].as_str().unwrap();
        assert!(stdout_sample.contains("subdir"));
    }
}

#[test]
fn term_run_with_sample_kb_limit() {
    let t = tmp();
    // Generate some output and limit sample size
    let v = run_json(t.path(), &["term","run","--sample-kb","1","echo","this is a test message that might be truncated"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    let data = &v["data"];
    assert_eq!(data["exit_code"], 0);
    
    // Sample should be limited but SHA should still be of full output
    assert!(data["stdout_sample"].as_str().unwrap().len() <= 1024);
    assert!(data["stdout_sha256"].as_str().unwrap().starts_with("sha256:"));
}

#[test]
fn term_run_captures_stderr() {
    let t = tmp();
    // Use a command that outputs to stderr
    #[cfg(unix)]
    let cmd_args = &["term","run","sh","-c","echo error >&2"];
    #[cfg(windows)]
    let cmd_args = &["term","run","cmd","/c","echo error 1>&2"];
    
    let v = run_json(t.path(), cmd_args);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    let data = &v["data"];
    assert_eq!(data["exit_code"], 0);
    assert!(data["stderr_sample"].as_str().unwrap().contains("error"));
    assert!(data["stderr_sha256"].as_str().unwrap().starts_with("sha256:"));
}

#[test]
fn term_run_complex_command() {
    let t = tmp();
    // Test command composition with multiple arguments
    let v = run_json(t.path(), &["term","run","echo","hello","world","from","nexoscli"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    let data = &v["data"];
    assert_eq!(data["exit_code"], 0);
    
    let stdout = data["stdout_sample"].as_str().unwrap();
    assert!(stdout.contains("hello"));
    assert!(stdout.contains("world"));
    assert!(stdout.contains("nexoscli"));
    
    // Command should be properly joined
    let cmd = data["cmd"].as_str().unwrap();
    assert!(cmd.contains("hello world from nexoscli"));
}

#[test]
fn term_run_escapes_special_characters() {
    let t = tmp();
    // Test with arguments that need shell escaping
    let v = run_json(t.path(), &["term","run","echo","hello; echo world"]);
    assert_eq!(v["errors"], serde_json::Value::Array(vec![]));
    let data = &v["data"];
    assert_eq!(data["exit_code"], 0);
    
    // Should treat the semicolon as literal text, not shell command separator
    let stdout = data["stdout_sample"].as_str().unwrap();
    assert!(stdout.contains("hello; echo world"));
}
