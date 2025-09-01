# Cline Plugin — Task History Integration (Phase 10)

Enhanced Cline task history import plugin for nexoscli with bulletproof cross-platform discovery and idempotent import capabilities.
Provides read-only integration with Cline AI task history across multiple editors with robust storage detection and SHA256-based change tracking.

---

## Phase 10 Enhancements

### Bulletproof Cross-Platform Discovery
- **Multi-layer discovery**: Environment overrides → devcontainer/remote paths → user globalStorage
- **Environment variable support**: 
  - `NEXOS_CLINE_STORAGE` - Override storage path discovery
  - `NEXOS_CLINE_TEST_ROOT` - Testing root directory override
- **Expanded editor support**: Code, Cursor, VSCodium, Windsurf, Code - Insiders, code-oss
- **Platform-agnostic**: Automatic platform detection and path resolution

### Idempotent Import with SHA256 Indexing
- **Change detection**: SHA256-based comparison prevents duplicate imports
- **State tracking**: Reports `created`, `changed`, `unchanged`, `busy`, `invalid` states
- **Busy file handling**: Uses `spin_sleep` to handle files being actively written
- **Atomic operations**: Safe concurrent access to task storage

### Doctor Integration
- **CI/CD integration**: `dev doctor --ci` includes Cline storage checks
- **Configurable enforcement**: Soft-fail default with `--require cline` strict mode
- **Environment variable support**: `NEXOS_CLINE_REQUIRED=1` for CI environments
- **Health reporting**: Validates Cline storage accessibility and task availability

## Commands

### `cline locate`
Find Cline storage locations across supported editors and platforms.

```bash
nexoscli cline locate                    # Find all Cline storage candidates
nexoscli cline locate --output json     # JSON format for automation
```

**JSON Output:**
```json
{
  "data": {
    "candidates": [
      {
        "editor": "Code",
        "path": "/Users/user/Library/Application Support/Code/User/globalStorage/continue.continue",
        "exists": true,
        "tasks_found": 42
      },
      {
        "editor": "Cursor", 
        "path": "/Users/user/Library/Application Support/Cursor/User/globalStorage/continue.continue",
        "exists": false,
        "tasks_found": 0
      }
    ],
    "selected": "/Users/user/Library/Application Support/Code/User/globalStorage/continue.continue"
  },
  "errors": [],
  "meta": { "run_id": "uuid" }
}
```

### `cline import`
Import tasks from Cline storage with redaction and filtering.

```bash
nexoscli cline import                            # Import from auto-detected storage
nexoscli cline import --storage /path/to/cline   # Import from specific path
nexoscli cline import --since 24h               # Import last 24 hours only
nexoscli cline import --since 7d                # Import last 7 days
nexoscli cline import --since 2025-09-01T00:00:00Z  # Import since specific time
nexoscli cline import --limit 100               # Limit to 100 most recent
nexoscli cline import --dry-run                 # Preview what would be imported
```

**JSON Output:**
```json
{
  "data": {
    "imported": 15,
    "skipped": 3,
    "redacted_keys": ["api_key", "token"],
    "output_dir": ".nexoscli/cline/tasks",
    "time_range": {
      "from": "2025-08-25T00:00:00Z",
      "to": "2025-09-01T15:30:45Z"
    }
  },
  "errors": [],
  "meta": { "run_id": "uuid" }
}
```

## Data Model

### Task Structure
```rust
pub struct ClineTask {
    pub id: String,                    // Cline session ID
    pub title: String,                 // Task description/title
    pub created_at: DateTime<Utc>,     // Task start time
    pub updated_at: DateTime<Utc>,     // Last activity time
    pub status: String,                // Task status from Cline
    pub messages: Vec<TaskMessage>,    // Conversation history
    pub metadata: TaskMetadata,        // Additional context
}

pub struct TaskMessage {
    pub role: String,                  // "user" | "assistant" | "system"
    pub content: String,               // Message content (redacted)
    pub timestamp: DateTime<Utc>,      // Message timestamp
    pub message_type: Option<String>,  // Message type if available
}

pub struct TaskMetadata {
    pub editor: String,                // Source editor (Code, Cursor, etc.)
    pub workspace: Option<String>,     // Workspace path if available
    pub total_messages: usize,         // Original message count
    pub redacted_fields: Vec<String>,  // List of redacted field types
}
```

## Cross-Platform Support

### Supported Editors
- **Visual Studio Code** (`Code`)
- **Visual Studio Code Insiders** (`Code - Insiders`)  
- **Cursor** - AI-powered code editor
- **VSCodium** - Open-source VS Code
- **Windsurf** - Codeium editor

### Storage Locations by Platform

#### macOS
```
~/Library/Application Support/{Editor}/User/globalStorage/continue.continue/
```

#### Linux  
```
~/.config/{Editor}/User/globalStorage/continue.continue/
```

#### Windows
```
%APPDATA%\{Editor}\User\globalStorage\continue.continue\
```

## Security & Redaction

### Automatic Redaction
The plugin automatically redacts sensitive information:

- **API Keys**: `api_key`, `apiKey`, `API_KEY`
- **Tokens**: `token`, `access_token`, `bearer_token`, `jwt`
- **Passwords**: `password`, `passwd`, `pwd`
- **Secrets**: `secret`, `private_key`, `client_secret`
- **Credentials**: Pattern-based detection of keys, tokens, passwords

### Redaction Process
```rust
fn redact_content(content: &str) -> String {
    // Replace sensitive values with "[REDACTED]"
    // Preserve structure while removing secrets
    // Log redacted field types for transparency
}
```

### Example Redaction
```json
// Before redaction
{
  "api_key": "sk-proj-abc123...",
  "message": "Use this token: ghp_xyz789..."
}

// After redaction  
{
  "api_key": "[REDACTED]",
  "message": "Use this token: [REDACTED]"
}
```

## Storage

### Import Structure
Tasks are imported into `.nexoscli/cline/tasks/` as normalized JSON files:

```
.nexoscli/
├── cline/
│   ├── tasks/
│   │   ├── task_20250901_143045_abc123.json
│   │   ├── task_20250901_152030_def456.json
│   │   └── ...
│   └── metadata.json        # Import metadata
└── log/
    └── events.jsonl         # All operations logged here
```

### File Naming Convention
```
task_{YYYYMMDD}_{HHMMSS}_{session_id}.json
```

### Metadata File
```json
{
  "last_import": "2025-09-01T15:30:45Z",
  "source_storage": "/path/to/cline/storage",
  "total_imported": 42,
  "redaction_stats": {
    "api_key": 5,
    "token": 8,
    "password": 2
  }
}
```

## AI Integration

### Available Tools
1. **`cline_locate`** - Find Cline storage locations
2. **`cline_import`** - Import tasks with filtering and redaction

### JSON Schemas
```rust
// cline_import schema
{
  "type": "object",
  "properties": {
    "storage": {"type": "string", "description": "Path to Cline storage directory"},
    "since": {"type": "string", "description": "Time filter: '24h', '7d', or RFC3339 timestamp"},
    "limit": {"type": "integer", "minimum": 1, "description": "Maximum tasks to import"},
    "dry_run": {"type": "boolean", "description": "Preview import without writing files"}
  }
}
```

### Agent Usage Examples
```bash
# AI can discover available Cline history
nexoscli cline locate --output json

# AI can import recent context for analysis
nexoscli cline import --since 24h --limit 50 --output json

# AI can preview imports safely
nexoscli cline import --dry-run --output json
```

## Implementation Details

### Storage Detection Algorithm
1. Enumerate known editor storage paths for current platform
2. Check for existence of `continue.continue` directory
3. Scan for task/session files within storage
4. Rank candidates by recency and task count
5. Select most recent active storage

### Time Filtering
- **Relative**: `24h`, `7d`, `30d` (hours/days from now)
- **Absolute**: RFC3339 timestamps (`2025-09-01T00:00:00Z`)
- **Default**: All available tasks (no filter)

### Error Handling
- Storage path not found or inaccessible
- Malformed Cline task files (skip with warning)
- Permission errors (graceful degradation)
- Disk space validation before large imports
- Atomic writes to prevent partial imports

### Event Logging
All operations generate structured events:
```json
{
  "plugin": "cline",
  "command": "import",
  "attrs": {
    "imported": 15,
    "source": "/path/to/storage",
    "time_filter": "24h"
  },
  "content_hash": "sha256:...",
  "args_hash": "sha256:..."
}
```

## Testing

### Unit Tests
- Cross-platform path detection
- Redaction algorithm validation
- Time filter parsing and application
- JSON schema compliance

### Integration Tests  
- Fake Cline storage creation and import
- Redaction verification
- Dry-run mode validation
- Time-based filtering accuracy
- Storage candidate detection

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_platform_paths() {
        // Test storage path detection across platforms
    }

    #[test]
    fn test_redaction_patterns() {
        // Verify sensitive data is properly redacted
    }

    #[test]
    fn test_time_filtering() {
        // Test relative and absolute time filters
    }
}
```

---

## Quick Examples

```bash
# Discover and import recent Cline tasks
nexoscli cline locate
nexoscli cline import --since 7d

# Preview import with filtering
nexoscli cline import --since 24h --limit 10 --dry-run

# Import from specific storage location  
nexoscli cline import --storage "/path/to/custom/cline"

# JSON workflow for automation
nexoscli cline import --output json | jq '.data.imported'
```

## Security Notes

- **Read-only**: Plugin only reads Cline data, never modifies source
- **Redaction**: Automatic removal of API keys, tokens, passwords
- **Local storage**: All imported data stays in local `.nexoscli/` directory
- **Transparency**: Redaction statistics reported in output

For general plugin architecture and development patterns, see [../README.md](../README.md).
