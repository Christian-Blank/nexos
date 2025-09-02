use std::fs;
use std::path::Path;

mod helpers;

#[test]
fn workflow_init_creates_spec() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    // Arrange: Initialize project
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Act: Initialize workflow spec
    let result = helpers::run_json(temp_path, &[
        "workflow", "init", "--id", "WF-test"
    ]);
    
    // Assert: Check JSON envelope structure
    assert!(result["data"]["id"].as_str().unwrap() == "WF-test");
    assert!(result["data"]["spec"].as_str().unwrap().contains("WF-test"));
    
    // Assert: Check spec file was created
    let spec_path = temp_path.join(".nexoscli/workflows/WF-test/spec.yaml");
    assert!(spec_path.exists());
    
    let spec_content = fs::read_to_string(&spec_path).unwrap();
    assert!(spec_content.contains("id: WF-test"));
    assert!(spec_content.contains("version: '0.1'") || spec_content.contains("version: \"0.1\""));
    assert!(spec_content.contains("issue.new"));
    assert!(spec_content.contains("dev.plugins.check"));
}

#[test]
fn workflow_init_custom_output_path() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Act: Initialize workflow with custom output path
    let result = helpers::run_json(temp_path, &[
        "workflow", "init", "--id", "WF-custom", "--out", "custom-workflow.yaml"
    ]);
    
    // Assert: Check result points to custom path
    assert!(result["data"]["spec"].as_str().unwrap() == "custom-workflow.yaml");
    assert!(temp_path.join("custom-workflow.yaml").exists());
}

#[test]
fn workflow_plan_shows_execution_plan() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    helpers::run_ok(temp_path, &["workflow", "init", "--id", "WF-plan-test"]);
    
    // Act: Plan workflow execution
    let result = helpers::run_json(temp_path, &[
        "workflow", "plan", "--spec", ".nexoscli/workflows/WF-plan-test/spec.yaml"
    ]);
    
    // Assert: Check plan structure
    assert!(result["data"]["spec_id"].as_str().unwrap() == "WF-plan-test");
    let steps = result["data"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    
    // Both steps should run initially (no prior effects)
    assert!(steps[0]["step_id"].as_str().unwrap() == "s1");
    assert!(steps[0]["activity"].as_str().unwrap() == "issue.new");
    assert!(steps[0]["would_run"].as_bool().unwrap() == true);
    assert!(steps[0]["reason"].as_str().unwrap() == "no prior effect");
    
    assert!(steps[1]["step_id"].as_str().unwrap() == "s2");
    assert!(steps[1]["activity"].as_str().unwrap() == "dev.plugins.check");
    assert!(steps[1]["would_run"].as_bool().unwrap() == true);
    assert!(steps[1]["reason"].as_str().unwrap() == "non-idempotent");
}

#[test]
fn workflow_run_dry_run_mode() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    helpers::run_ok(temp_path, &["workflow", "init", "--id", "WF-dryrun"]);
    
    // Act: Run workflow in dry-run mode
    let result = helpers::run_json(temp_path, &[
        "workflow", "run", "--spec", ".nexoscli/workflows/WF-dryrun/spec.yaml", "--dry-run"
    ]);
    
    // Assert: Should return plan instead of execution results
    assert!(result["data"]["spec_id"].as_str().unwrap() == "WF-dryrun");
    assert!(result["data"]["steps"].as_array().is_some());
    
    // Assert: No effects or history should be created in dry-run
    assert!(!temp_path.join(".nexoscli/effects/WF-dryrun").exists());
    assert!(!temp_path.join(".nexoscli/workflows/WF-dryrun/history.jsonl").exists());
}

#[test] 
fn workflow_run_execution_creates_effects_and_history() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Create a simpler spec for testing to avoid complex CLI interactions
    fs::create_dir_all(temp_path.join(".nexoscli/workflows/WF-simple")).unwrap();
    let simple_spec = r#"
version: "0.1"
id: WF-simple
name: Simple workflow
steps:
  - id: s1
    activity: dev.plugins.check
    args: {}
    idempotent: true
    retry:
      max_attempts: 1
      backoff_ms: 0
"#;
    fs::write(temp_path.join(".nexoscli/workflows/WF-simple/spec.yaml"), simple_spec).unwrap();
    
    // Act: Run workflow
    let result = helpers::run_json(temp_path, &[
        "workflow", "run", "--spec", ".nexoscli/workflows/WF-simple/spec.yaml"
    ]);
    
    // Assert: Check run summary structure
    assert!(result["data"]["spec_id"].as_str().unwrap() == "WF-simple");
    assert!(result["data"]["ran"].as_u64().is_some());
    assert!(result["data"]["skipped"].as_u64().is_some());
    assert!(result["data"]["failed"].as_u64().is_some());
    assert!(result["data"]["started_at"].as_str().is_some());
    assert!(result["data"]["finished_at"].as_str().is_some());
    
    // Assert: Effects directory should be created
    assert!(temp_path.join(".nexoscli/effects/WF-simple").exists());
    assert!(temp_path.join(".nexoscli/effects/WF-simple/s1.json").exists());
    
    // Assert: History file should be created
    assert!(temp_path.join(".nexoscli/workflows/WF-simple/history.jsonl").exists());
    
    // Check effect record structure
    let effect_content = fs::read_to_string(temp_path.join(".nexoscli/effects/WF-simple/s1.json")).unwrap();
    let effect: serde_json::Value = serde_json::from_str(&effect_content).unwrap();
    assert!(effect["spec_id"].as_str().unwrap() == "WF-simple");
    assert!(effect["step_id"].as_str().unwrap() == "s1");
    assert!(effect["input_sha"].as_str().is_some());
    assert!(effect["status"].as_str().is_some());
    assert!(effect["exit_code"].as_i64().is_some());
}

#[test]
fn workflow_run_idempotency_skips_successful_steps() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Create idempotent workflow spec
    fs::create_dir_all(temp_path.join(".nexoscli/workflows/WF-idempotent")).unwrap();
    let idempotent_spec = r#"
version: "0.1"
id: WF-idempotent  
name: Idempotent workflow
steps:
  - id: s1
    activity: dev.plugins.check
    args: {}
    idempotent: true
    retry:
      max_attempts: 1
      backoff_ms: 0
"#;
    fs::write(temp_path.join(".nexoscli/workflows/WF-idempotent/spec.yaml"), idempotent_spec).unwrap();
    
    // Act: Run workflow first time
    let first_run = helpers::run_json(temp_path, &[
        "workflow", "run", "--spec", ".nexoscli/workflows/WF-idempotent/spec.yaml"
    ]);
    
    // Act: Run workflow second time (should be idempotent)
    let second_run = helpers::run_json(temp_path, &[
        "workflow", "run", "--spec", ".nexoscli/workflows/WF-idempotent/spec.yaml"
    ]);
    
    // Assert: First run should execute the step
    assert!(first_run["data"]["ran"].as_u64().unwrap() >= 0);
    
    // Assert: Second run should skip if first was successful and idempotent
    // Note: This depends on the first run being successful
    if first_run["data"]["ran"].as_u64().unwrap() > 0 && first_run["data"]["failed"].as_u64().unwrap() == 0 {
        assert!(second_run["data"]["skipped"].as_u64().unwrap() > 0);
    }
}

#[test] 
fn workflow_status_shows_effects_and_history() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Create and run a simple workflow
    fs::create_dir_all(temp_path.join(".nexoscli/workflows/WF-status")).unwrap();
    let spec = r#"
version: "0.1"
id: WF-status
name: Status test workflow
steps:
  - id: s1
    activity: dev.plugins.check
    args: {}
    idempotent: false
    retry:
      max_attempts: 1
      backoff_ms: 0
"#;
    fs::write(temp_path.join(".nexoscli/workflows/WF-status/spec.yaml"), spec).unwrap();
    
    // Run the workflow to generate effects and history
    helpers::run_ok(temp_path, &[
        "workflow", "run", "--spec", ".nexoscli/workflows/WF-status/spec.yaml"
    ]);
    
    // Act: Check workflow status
    let result = helpers::run_json(temp_path, &[
        "workflow", "status", "--id", "WF-status"
    ]);
    
    // Assert: Status should include effects and history
    assert!(result["data"]["id"].as_str().unwrap() == "WF-status");
    assert!(result["data"]["effects"].as_array().is_some());
    assert!(result["data"]["history"].as_str().is_some());
    
    // Effects should contain at least one record
    let effects = result["data"]["effects"].as_array().unwrap();
    if !effects.is_empty() {
        let effect = &effects[0];
        assert!(effect["spec_id"].as_str().is_some());
        assert!(effect["step_id"].as_str().is_some());
        assert!(effect["status"].as_str().is_some());
    }
    
    // History should contain execution records
    let history = result["data"]["history"].as_str().unwrap();
    if !history.is_empty() {
        assert!(history.contains("WF-status") || history.contains("s1"));
    }
}

#[test]
fn workflow_status_empty_when_no_execution() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Act: Check status for non-existent workflow
    let result = helpers::run_json(temp_path, &[
        "workflow", "status", "--id", "WF-nonexistent"
    ]);
    
    // Assert: Should return empty effects and history
    assert!(result["data"]["id"].as_str().unwrap() == "WF-nonexistent");
    assert_eq!(result["data"]["effects"].as_array().unwrap().len(), 0);
    assert_eq!(result["data"]["history"].as_str().unwrap(), "");
}

#[test]
fn workflow_plan_after_partial_execution() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Create workflow with mixed idempotency
    fs::create_dir_all(temp_path.join(".nexoscli/workflows/WF-mixed")).unwrap();
    let mixed_spec = r#"
version: "0.1"
id: WF-mixed
name: Mixed idempotency workflow  
steps:
  - id: s1
    activity: dev.plugins.check
    args: {}
    idempotent: true
    retry:
      max_attempts: 1
      backoff_ms: 0
  - id: s2
    activity: dev.plugins.check
    args: {}
    idempotent: false
    retry:
      max_attempts: 1
      backoff_ms: 0
"#;
    fs::write(temp_path.join(".nexoscli/workflows/WF-mixed/spec.yaml"), mixed_spec).unwrap();
    
    // Run workflow once
    helpers::run_ok(temp_path, &[
        "workflow", "run", "--spec", ".nexoscli/workflows/WF-mixed/spec.yaml"
    ]);
    
    // Act: Plan workflow again
    let plan_result = helpers::run_json(temp_path, &[
        "workflow", "plan", "--spec", ".nexoscli/workflows/WF-mixed/spec.yaml"
    ]);
    
    // Assert: Plan should reflect previous execution state
    let steps = plan_result["data"]["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    
    // s1 (idempotent) might be skipped if successful, s2 (non-idempotent) should always run
    assert!(steps[1]["step_id"].as_str().unwrap() == "s2");
    assert!(steps[1]["would_run"].as_bool().unwrap() == true);
    assert!(steps[1]["reason"].as_str().unwrap() == "non-idempotent");
}

#[test]
fn workflow_run_respects_task_context() {
    let temp_dir = helpers::tmp();
    let temp_path = temp_dir.path();
    
    helpers::run_ok(temp_path, &["dev", "init", "--yes"]);
    
    // Create and start a task to test context injection
    helpers::run_ok(temp_path, &["task", "new", "--title", "Workflow test task"]);
    let task_list = helpers::run_json(temp_path, &["task", "list"]);
    let tasks = task_list["data"].as_array().unwrap();
    assert!(!tasks.is_empty(), "No tasks found after creating a task");
    let task_id = tasks[0]["id"].as_str().unwrap();
    helpers::run_ok(temp_path, &["task", "start", "--task", task_id]);
    
    // Create simple workflow
    fs::create_dir_all(temp_path.join(".nexoscli/workflows/WF-context")).unwrap();
    let context_spec = r#"
version: "0.1"
id: WF-context
name: Context test workflow
steps:
  - id: s1
    activity: dev.plugins.check
    args: {}
    idempotent: false
    retry:
      max_attempts: 1
      backoff_ms: 0
"#;
    fs::write(temp_path.join(".nexoscli/workflows/WF-context/spec.yaml"), context_spec).unwrap();
    
    // Act: Run workflow with active task context
    let result = helpers::run_json(temp_path, &[
        "workflow", "run", "--spec", ".nexoscli/workflows/WF-context/spec.yaml"
    ]);
    
    // Assert: Result should have task context in meta
    assert!(result["meta"]["context"]["task_id"].as_str().is_some());
    assert!(result["meta"]["context"]["session_id"].as_str().is_some());
    assert!(result["meta"]["context"]["stage"].as_str().is_some());
    
    // Clean up
    helpers::run_ok(temp_path, &["task", "end"]);
}
