---
schema: readme.v0
title: "task plugin"
summary: "Task lifecycle: new/start/pause/resume/stage/status/list/end/bundle."
kind: leaf
owner: "@plugins/dri"
status: live
version: "0.1.0"
last_reviewed: "2025-09-06T22:06:30Z"
parents: ["../README.md","../../README.md"]
children: []
source_of_truth:
  - "./src/lib.rs"
  - "./README.md"
in_scope:
  - "User-facing behavior and CLI surface of task plugin"
out_of_scope:
  - "Workflow engine internals (see plugins/workflow)"
inputs:
  - "Events log"
outputs:
  - ".nexoscli/tasks/*, envelopes"
tags: ["plugin","tasks"]
auto: { }
---
# Task Plugin

Task lifecycle management with automatic context injection.

## Overview

The task plugin provides complete task lifecycle management for NEXOS projects. When a task is active, all CLI commands automatically get tagged with task context (`{task_id, stage}`) in both event logs and JSON envelopes.

## Commands

### Basic Lifecycle
- `task new --title "Task title" [--id TASK-ID]` — Create a new task
- `task start --task TASK-ID` — Start a task (make it active)
- `task pause [--task TASK-ID]` — Pause the active task
- `task resume --task TASK-ID` — Resume a paused task
- `task end [--task TASK-ID]` — End the current/specified task

### Task Management  
- `task stage STAGE [--task TASK-ID]` — Set stage: planning|implementation|verification|done|blocked
- `task status [--task TASK-ID]` — Show task status with session history and event counts
- `task list` — List all tasks

### TracePack Integration
- `task bundle --task TASK-ID --out PATH [--include CSV]` — Create TracePack bundle

## Context Injection

**Key Innovation**: When a task is active, ALL CLI commands automatically receive task context without any changes to existing plugins.

### Example Workflow

```bash
# Create and start a task
./target/debug/nexoscli task new --title "Implement user auth"
./target/debug/nexoscli task start --task TASK-20250902-implement-user-auth

# Any command now gets automatic context injection
./target/debug/nexoscli issue new --title "Add login form"
# Returns with context: {"task_id": "TASK-...", "stage": "planning"}

# Change stage
./target/debug/nexoscli task stage implementation --task TASK-20250902-implement-user-auth

# Commands now get updated context
./target/debug/nexoscli term run "npm test"
# Event and envelope tagged with {"task_id": "TASK-...", "stage": "implementation"}

# End the task
./target/debug/nexoscli task end --task TASK-20250902-implement-user-auth
```

## Task Stages

- **planning** — Initial analysis and design
- **implementation** — Active development
- **verification** — Testing and validation  
- **done** — Completed successfully
- **blocked** — Waiting on external dependencies

## Bundle Contents

Task bundles include:
- `task/task.json` — Task ledger with sessions and metadata
- `events.ndjson` — All events tagged with the task_id
- `patches/` — Git diffs from `nexoscli term run` commands
- `status/` — Git status snapshots
- `cline/tasks/` — Cline tasks within the time window (±5min)
- `manifest.json` — Bundle metadata and file inventory
- `README.md` — Bundle documentation

## Task Record Structure

```json
{
  "id": "TASK-20250902-implement-user-auth",
  "title": "Implement user auth",
  "stage": "implementation", 
  "status": "active",
  "created_at": "2025-09-02T08:30:00Z",
  "started_at": "2025-09-02T08:31:00Z",
  "sessions": [
    {
      "start": "2025-09-02T08:31:00Z",
      "end": null
    }
  ]
}
```

## Concurrent Task Prevention

Only one task can be active at a time. Starting a new task requires pausing the current active task first.

## File Storage

- **Tasks**: `.nexoscli/tasks/{task_id}/task.json`
- **Current**: `.nexoscli/task/current` (contains active task ID)
- **Events**: Context-tagged events in `.nexoscli/log/events.jsonl`

## Implementation Notes

- Task plugin implements the standard `Plugin` trait with `name() = "task"`
- Context injection happens transparently via the core `CommandContext.active_task_map()` method
- No changes required to existing plugins - context is injected automatically
- Event counting and bundle generation use the actual events.jsonl file for accuracy

## Commands

<!-- BEGIN: AUTO (id=commands) -->
- task bundle — Build a TracePack bundle for a task window
- task end — End the current (or given) task
- task list — List tasks
- task new — Create a new task
- task pause — Pause the active task
- task resume — Resume a paused task
- task stage — Set the stage for a task
- task start — Start a task (make it active)
- task status — Show task status
<!-- END: AUTO -->
