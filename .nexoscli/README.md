# .nexoscli — Runtime Data Directory

Local data directory created by nexoscli for persistent storage and event logging.
This directory is automatically managed and should not be edited manually.

---

## Directory Structure

```
.nexoscli/
├── README.md          # This file (auto-generated)
├── issues/            # Issue storage (YAML files)
│   ├── ISS-0001.yaml
│   ├── ISS-0002.yaml
│   ├── .counter       # Atomic ID counter
│   └── ...
└── log/               # Event logging
    └── events.jsonl   # Append-only event stream
```

## Data Storage

### Issues Directory (`issues/`)
Local issue tracking storage:
- **Individual YAML files** per issue (`ISS-NNNN.yaml`)
- **Atomic counter** (`.counter`) for ID generation
- **Human-readable** format for inspection and recovery

**Example issue file:**
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

### Event Log (`log/events.jsonl`)
Append-only audit trail:
- **One JSON object per line** (JSON Lines format)
- **All CLI operations** are logged with metadata
- **SHA256 hashes** for content integrity
- **Replay capability** for debugging and analytics

**Example event entry:**
```json
{
  "ts": "2025-09-01T12:34:56Z",
  "run_id": "uuid-goes-here", 
  "plugin": "issue",
  "command": "new",
  "args_hash": "sha256:abc123...",
  "content_hash": "sha256:def456...",
  "exit_code": 0,
  "attrs": {"id": "ISS-0001", "kind": "bug"}
}
```

## Purpose

### Data Persistence
- **Issue tracking**: Local storage without external dependencies
- **Configuration**: Future plugin-specific settings
- **Cache**: Performance optimization for repeated operations

### Observability
- **Audit trail**: Complete history of all operations
- **Debugging**: Rich context for troubleshooting
- **Analytics**: Usage patterns and performance metrics
- **Compliance**: Immutable operation log

### Recovery
- **Human-readable**: YAML files can be inspected and recovered
- **Content integrity**: SHA256 hashes prevent corruption
- **Event replay**: Reconstruct system state from event log

## File Management

### Automatic Creation
Directory and files are created automatically on first use:
```bash
nexoscli issue new --title "First issue" --kind feature
# Creates .nexoscli/issues/ISS-0001.yaml
# Appends to .nexoscli/log/events.jsonl
```

### Manual Inspection
Safe to read, but avoid manual editing:
```bash
# View recent events
tail -f .nexoscli/log/events.jsonl

# Inspect specific issue  
cat .nexoscli/issues/ISS-0001.yaml

# List all issues
ls -la .nexoscli/issues/
```

### Backup and Migration
```bash
# Backup entire data directory
tar -czf nexoscli-backup-$(date +%Y%m%d).tar.gz .nexoscli/

# Restore from backup
tar -xzf nexoscli-backup-20250901.tar.gz
```

## Security Considerations

### File Permissions
- **User-only access**: `700` permissions on `.nexoscli/`
- **Read-write**: Issue files are user-readable and writable
- **Append-only**: Event log prevents accidental deletion

### Content Hashing
- **Integrity verification**: SHA256 hashes detect tampering
- **Reproducibility**: Content hashes enable deterministic operations
- **Audit chain**: Hash links provide tamper evidence

### Privacy
- **Local-only**: No external network dependencies
- **Issue content**: May contain sensitive project information
- **Event metadata**: Includes command arguments and timing

## Troubleshooting

### Corrupted Issue Files
```bash
# Check YAML validity
nexoscli issue show ISS-0001
# If corrupted, check git history or backups
```

### Missing Counter File
```bash
# Will be recreated automatically on next issue creation
nexoscli issue new --title "Test" --kind task
```

### Large Event Log
```bash
# Archive old events (optional)
gzip .nexoscli/log/events.jsonl
# Creates events.jsonl.gz, log continues in new events.jsonl
```

### Permission Issues
```bash
# Reset permissions
chmod -R 700 .nexoscli/
```

---

## Integration

### Git Handling
Add to `.gitignore` for project repositories:
```gitignore
# Local nexoscli data
.nexoscli/
```

### IDE Support
Most IDEs will ignore this directory automatically due to the `.` prefix.

### CI/CD
Event logs can be collected for build analytics:
```bash
# In CI pipeline
cat .nexoscli/log/events.jsonl | jq '.exit_code' | sort | uniq -c
```

For CLI usage and commands, see [nexoscli-bin/README.md](nexoscli-bin/README.md).
For plugin development affecting storage, see [plugins/README.md](plugins/README.md).
