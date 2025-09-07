# Task Workflows

Note on Plans (PLAN-34):
- Starting a plan via `nexoscli dev plans start` automatically creates and starts a task, updates the plan’s front‑matter, and moves it to `docs/project/current/`.
- Use the `task` commands below normally to pause/resume/stage/end/bundle as you work.
- Finalize plans with `nexoscli dev plans finalize` (records `implemented_in`, ends task + bundle best‑effort, moves to `docs/project/done/`).

Plan lifecycle checks (PLAN-50):
- Verify lifecycle invariants deterministically:
  ```
  nexoscli dev plans verify --output json
  ```
- Build deterministic README indexes (AUTO blocks with id=plans-index) across docs/project/*:
  ```
  nexoscli dev plans index build --output json
  ```
  Use `--dry-run` to preview changes without writing; indexes are idempotent and sorted with relative links.
- CI integration (doctor):
  ```
  nexoscli dev doctor --ci --checks plans --output json
  ```
  When `--checks plans` is provided explicitly, the plans check is treated as required and will fail CI on violations.

NEXOS provides workflow engine abstraction for task lifecycle management, allowing you to run task transitions either locally or prepare Temporal workflow envelopes.

## Engine Selection

You can run `task` transitions with different execution engines:

- **Local (default)**: Runs gates/hooks as defined in `workflows/task.yaml` immediately during command execution
- **Temporal**: Writes Start/Signal envelopes to `.nexoscli/engine/outbox/` and **does not** run hooks locally

### Local Engine

The local engine preserves existing behavior and executes any configured gates and hooks directly during task transitions.

```bash
# Default behavior (local engine)
nexoscli task start --task TASK-20250902-example
nexoscli task stage implementation --task TASK-20250902-example
nexoscli task end --task TASK-20250902-example

# Explicit local engine
nexoscli task start --task TASK-20250902-example --engine local
```

### Temporal Engine

The temporal engine writes workflow envelopes to the outbox directory for later consumption by Temporal workers.

```bash
# Use temporal engine
nexoscli task start --task TASK-20250902-example --engine temporal
nexoscli task stage verification --engine temporal
nexoscli task end --engine temporal
```

## Outbox Format

When using `--engine temporal`, envelopes are written to `.nexoscli/engine/outbox/` with the following structure:

```json
{
  "op": "start|signal|query",
  "workflow": "task", 
  "key": "TASK-20250902-example",
  "name": "stage_enter|end|null",
  "attrs": {
    "task_id": "TASK-20250902-example",
    "stage": "implementation", 
    "status": "active",
    "session_id": "SES-abc123def456",
    "repo": "/path/to/repo",
    "branch": "main"
  },
  "body": {
    "action": "start|stage_enter|end",
    "stage": "implementation"
  },
  "ts": "2025-09-02T12:34:56Z"
}
```

### Search Attributes

The temporal engine automatically includes these search attributes in workflow envelopes:

- `task_id`: Task identifier
- `stage`: Current task stage (planning|implementation|verification|done|blocked)  
- `status`: Task status (new|active|paused|ended)
- `session_id`: Session identifier for granular tracking
- `repo`: Git repository path (if available)
- `branch`: Git branch name (if available)

## Workflow Operations

### Start Operation

Creates a new workflow for the task:

```bash
nexoscli task start --task TASK-20250902-example --engine temporal
```

Generates a workflow start envelope with action "start".

### Stage Transitions

Sends signals to update workflow state:

```bash
nexoscli task stage implementation --engine temporal
```

Generates a signal envelope with:
- Signal name: "stage_enter" 
- Body includes action "stage_enter" and target stage

### End Operation

Sends final signal to complete the workflow:

```bash
nexoscli task end --engine temporal  
```

Generates a signal envelope with action "end".

## Integration with Existing Workflows

Local engine behavior is unchanged - any existing `workflows/task.yaml` configurations continue to work as before. The temporal engine provides an additional execution path for future Temporal integration.

Example workflow configuration (local engine only):

```yaml
version: 0.1
hooks:
  on_enter:
    planning:
      - create: ["docs/requirements-<task_id>.md"]
    implementation: 
      - create: ["src/components/<task_id>/"]
```

## Best Practices

1. **Development**: Use local engine for immediate feedback during development
2. **Production**: Use temporal engine when integrating with Temporal workflow systems  
3. **Inspection**: Check `.nexoscli/engine/outbox/` to inspect temporal envelopes before sending
4. **Cleanup**: Temporal envelopes accumulate in the outbox - implement cleanup as needed
5. **Fallback**: Local engine provides reliable fallback if temporal integration is unavailable
