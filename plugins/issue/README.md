# Issue Plugin — Local Issue Tracking

Local issue management plugin for nexoscli.
Provides git-like issue tracking without external dependencies.

---

## Commands

### `issue new`
Create a new issue with structured metadata.

```bash
nexoscli issue new --title "Fix validation bug" --kind bug
nexoscli issue new --title "Add user auth" --kind feature --component auth
```

**JSON Output:**
```json
{
  "data": {
    "id": "ISS-0001",
    "title": "Fix validation bug",
    "kind": "bug",
    "status": "open",
    "created_at": "2025-09-01T12:34:56Z"
  },
  "errors": [],
  "meta": { "run_id": "uuid" }
}
```

### `issue list`
List issues with filtering and formatting options.

```bash
nexoscli issue list                    # All issues
nexoscli issue list --status open     # Open issues only
nexoscli issue list --kind bug        # Bug issues only
nexoscli issue list --output json     # JSON format
```

**JSON Output:**
```json
{
  "data": {
    "issues": [
      {
        "id": "ISS-0001",
        "title": "Fix validation bug",
        "kind": "bug",
        "status": "open",
        "created_at": "2025-09-01T12:34:56Z"
      }
    ],
    "total": 1
  },
  "errors": [],
  "meta": { "run_id": "uuid" }
}
```

### `issue show`
Display detailed information about a specific issue.

```bash
nexoscli issue show ISS-0001
nexoscli issue show ISS-0001 --output json
```

### `issue close`
Close an issue and record closure metadata.

```bash
nexoscli issue close ISS-0001
nexoscli issue close ISS-0001 --reason "Fixed in commit abc123"
```

## Data Model

### Issue Structure
```rust
pub struct Issue {
    pub id: String,           // Auto-generated: ISS-NNNN
    pub title: String,        // Required, 1-200 chars
    pub description: Option<String>,
    pub kind: IssueKind,      // bug | feature | task | epic
    pub status: IssueStatus,  // open | closed
    pub component: Option<String>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}
```

### Issue Kinds
- **`bug`**: Defects or incorrect behavior
- **`feature`**: New functionality requests  
- **`task`**: General work items
- **`epic`**: Large features spanning multiple issues

## Storage

Issues are stored as individual YAML files in `.nexoscli/issues/`:
```
.nexoscli/
├── issues/
│   ├── ISS-0001.yaml
│   ├── ISS-0002.yaml
│   └── ...
└── log/
    └── events.jsonl       # All operations logged here
```

### File Format
```yaml
---
id: ISS-0001
title: "Fix validation bug"
description: |
  The validation function incorrectly rejects valid input
  when special characters are present.
kind: bug
status: open
component: validation
created_at: "2025-09-01T12:34:56Z"
metadata:
  reporter: system
  priority: high
```

## AI Integration

The issue plugin exposes comprehensive tool specifications for AI agents:

### Available Tools
1. **`issue_new`** - Create new issues
2. **`issue_list`** - Query and filter issues  
3. **`issue_show`** - Get issue details
4. **`issue_close`** - Close issues with reason

### JSON Schemas
Each command has a full JSON Schema for validation:
```rust
// Example for issue_new
{
  "type": "object",
  "properties": {
    "title": {"type": "string", "minLength": 1, "maxLength": 200},
    "kind": {"type": "string", "enum": ["bug", "feature", "task", "epic"]},
    "component": {"type": "string"},
    "description": {"type": "string"}
  },
  "required": ["title", "kind"]
}
```

### Agent Usage Examples
```bash
# AI can create issues based on analysis
nexoscli issue new --title "Memory leak in parser" --kind bug --component parsing

# AI can query for patterns
nexoscli issue list --kind bug --output json | jq '.data.issues[] | select(.component == "parsing")'
```

## Implementation Details

### ID Generation
- Sequential numbering: `ISS-0001`, `ISS-0002`, etc.
- Thread-safe atomic increment
- Persisted in `.nexoscli/issues/.counter`

### Error Handling  
- Validation errors for malformed input
- File system errors for storage operations
- Duplicate ID detection and recovery
- Graceful handling of corrupted issue files

### Event Logging
All operations generate structured events:
```json
{
  "plugin": "issue",
  "command": "new", 
  "attrs": {"id": "ISS-0001", "kind": "bug"},
  "content_hash": "sha256:...",  // Hash of issue content
  "args_hash": "sha256:..."      // Hash of CLI args
}
```

## Testing

### Unit Tests
- Issue creation and validation
- File I/O operations  
- ID generation logic
- JSON schema compliance

### Integration Tests
- End-to-end command execution
- JSON contract verification
- Event log validation
- Error scenario handling

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_creation() {
        // Test issue creation with valid data
    }

    #[test] 
    fn test_json_output_contract() {
        // Verify JSON output matches expected schema
    }
}
```

---

## Quick Examples

```bash
# Create and work with issues
nexoscli issue new --title "Add user authentication" --kind feature --component auth
nexoscli issue list --status open
nexoscli issue show ISS-0001
nexoscli issue close ISS-0001 --reason "Implemented in v1.2"

# JSON workflow for automation
nexoscli issue list --output json | jq '.data.issues[] | select(.kind == "bug")'
```

For general plugin architecture and development patterns, see [../README.md](../README.md).
