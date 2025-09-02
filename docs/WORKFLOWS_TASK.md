# Task Workflows

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
