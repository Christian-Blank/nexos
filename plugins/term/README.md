# Term Plugin — Command Provenance Tracking

Terminal command wrapper plugin for nexoscli.
Provides full provenance tracking for command execution with output capture, digest generation, and comprehensive observability.

---

## Commands

### `term run`
Execute a command with full provenance tracking and output capture.

```bash
nexoscli term run echo "hello world"                    # Basic command execution
nexoscli term run --cwd /tmp pwd                       # Custom working directory
nexoscli term run --sample-kb 1 cat large_file.txt    # Limit output sampling
nexoscli term run --passthrough npm test               # Also print output to console
nexoscli term run -- ls -la                           # Use -- for complex commands
```

**JSON Output:**
```json
{
  "data": {
    "cmd": "echo 'hello world'",
    "cwd": "/Users/user/project",
    "exit_code": 0,
    "stdout_sha256": "sha256:a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3",
    "stderr_sha256": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "stdout_sample": "hello world\n",
    "stderr_sample": ""
  },
  "errors": [],
  "meta": { "run_id": "uuid", "duration_ms": 12 }
}
```

## Data Model

### Command Result Structure
```rust
pub struct RunResult {
    pub cmd: String,                   // Executed command (shell-escaped)
    pub cwd: String,                   // Working directory
    pub exit_code: i32,                // Process exit code
    pub stdout_sha256: String,         // SHA-256 digest of full stdout
    pub stderr_sha256: String,         // SHA-256 digest of full stderr  
    pub stdout_sample: String,         // Truncated stdout sample
    pub stderr_sample: String,         // Truncated stderr sample
}
```

### Command Arguments
```rust
pub struct RunArgs {
    pub cmd: Vec<String>,              // Command and arguments
    pub cwd: Option<String>,           // Working directory override
    pub sample_kb: Option<usize>,      // Output truncation limit (KB)
}
```

## Cross-Platform Support

### Shell Execution
The plugin uses appropriate shell interpreters per platform:

#### Unix/Linux/macOS
```bash
sh -lc "command with args"
```
- Uses login shell (`-l`) for full environment
- Command context (`-c`) for single command execution
- Proper shell escaping for arguments with spaces/special characters

#### Windows  
```cmd
cmd /C "command with args"
```
- Uses Windows Command Prompt
- `/C` flag executes command and terminates
- Windows-compatible argument escaping

### Shell Escaping
```rust
fn sh_join(parts: &[String]) -> String {
    // Safe argument joining with shell escaping
    // Alphanumeric and safe chars: pass through
    // Special chars: wrap in single quotes with proper escaping
}
```

## Output Handling

### Digest Generation
All output is hashed using SHA-256 for integrity verification:

```rust
fn digest_and_sample(bytes: &[u8], kb: usize) -> (String, String) {
    // Generate SHA-256 hash of complete output
    let hash = format!("sha256:{:x}", Sha256::new().chain_update(bytes).finalize());
    
    // Create truncated sample for display
    let limit = (kb * 1024).min(bytes.len());
    let sample = String::from_utf8_lossy(&bytes[..limit]).to_string();
    
    (sample, hash)
}
```

### Sample Truncation
- **Default**: 64 KB sample limit
- **Configurable**: `--sample-kb N` to adjust limit
- **Full integrity**: Complete output always hashed regardless of sample size
- **UTF-8 safe**: Truncation respects character boundaries

### Output Modes
- **Standard**: Capture only (no console output)  
- **Passthrough**: `--passthrough` flag also prints to console
- **Both streams**: Separate capture of stdout and stderr

## CLI Integration

### Exit Code Propagation
The term plugin returns the wrapped command's exit code:
```bash
nexoscli term run false    # Returns exit code 1 (command failed)
nexoscli term run true     # Returns exit code 0 (command succeeded)
```

### Argument Handling
```bash
# Simple commands
nexoscli term run echo hello

# Commands with flags (use -- separator for clarity)
nexoscli term run -- ls -la --color=auto

# Complex commands with pipes/redirection (quoted)
nexoscli term run 'ls | grep test'
```

## Security & Safety

### Command Execution
- **Sandboxed**: Commands run in specified or current working directory
- **Shell isolation**: Each command runs in fresh shell instance  
- **No injection**: Proper shell escaping prevents command injection
- **User context**: Commands run with current user permissions

### Output Sanitization
- **Binary safe**: Handles binary output without corruption
- **UTF-8 conversion**: Lossy conversion for display compatibility
- **Size limits**: Prevents memory exhaustion on large outputs
- **Separate streams**: stdout/stderr captured independently

## AI Integration

### Available Tools
1. **`term_run`** - Execute commands with full provenance tracking

### JSON Schemas
```rust
// term_run schema
{
  "type": "object",
  "properties": {
    "cmd": {
      "type": "array",
      "items": {"type": "string"},
      "description": "Command and arguments to execute"
    },
    "cwd": {"type": "string", "description": "Working directory"},
    "sample_kb": {"type": "integer", "minimum": 1, "description": "Output sample size in KB"},
    "passthrough": {"type": "boolean", "description": "Also print output to console"}
  },
  "required": ["cmd"]
}
```

### Agent Usage Examples
```bash
# AI can execute commands and verify output
nexoscli term run --output json npm test | jq '.data.exit_code'

# AI can run commands in specific directories  
nexoscli term run --cwd /project --output json make build

# AI can capture command output for analysis
nexoscli term run git status --output json | jq '.data.stdout_sample'
```

## Implementation Details

### Command Composition
1. Parse command line arguments into command vector
2. Apply shell-safe escaping to individual arguments
3. Join into single shell command string
4. Execute via platform-appropriate shell

### Process Management
```rust
let mut cmd = if cfg!(target_os = "windows") {
    let mut c = StdCommand::new("cmd");
    c.arg("/C").arg(&joined_command);
    c
} else {
    let mut c = StdCommand::new("sh");
    c.arg("-lc").arg(&joined_command);
    c
};

let output = cmd.current_dir(&cwd).output()?;
```

### Error Handling
- **Command not found**: Shell reports command not found
- **Permission denied**: System permission errors passed through
- **Working directory**: Invalid CWD returns appropriate error
- **Resource limits**: System resource exhaustion handled gracefully
- **Signal interruption**: Process signals handled by shell

### Event Logging
All command executions generate structured events:
```json
{
  "plugin": "term",
  "command": "run",
  "attrs": {
    "cmd": "echo hello",
    "exit_code": 0,
    "cwd": "/project"
  },
  "content_hash": "sha256:...",    // Hash of command result
  "args_hash": "sha256:...",       // Hash of CLI arguments
  "dur_ms": 125                    // Command execution duration
}
```

## Performance & Observability

### Duration Tracking
- **Wall-clock timing**: Full command execution duration
- **Millisecond precision**: High-resolution timing
- **Event integration**: Duration included in event logs
- **Telemetry ready**: Metrics available for performance analysis

### Memory Management
- **Streaming capture**: Output captured without loading entire result in memory
- **Sample truncation**: Prevents memory exhaustion on large outputs
- **Efficient hashing**: SHA-256 computed during capture, not post-processing

## Testing

### Unit Tests
- Shell escaping and command composition
- Cross-platform shell selection  
- Output digest generation accuracy
- Sample truncation logic

### Integration Tests
- Command execution with various exit codes
- Working directory changes and validation
- Output capture for stdout and stderr
- Complex command handling with special characters
- Passthrough mode verification

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escaping() {
        // Test shell-safe argument escaping
    }

    #[test]
    fn test_output_capture() {
        // Verify stdout/stderr capture and digest generation
    }

    #[test]
    fn test_exit_code_propagation() {
        // Ensure exit codes are properly returned
    }
}
```

---

## Quick Examples

```bash
# Basic command execution
nexoscli term run echo "hello world"
nexoscli term run --output json date

# Development workflows
nexoscli term run --cwd /project make build
nexoscli term run --passthrough npm test

# System administration  
nexoscli term run --sample-kb 1 df -h
nexoscli term run -- ps aux | grep python

# Automation and scripting
CMD_RESULT=$(nexoscli term run --output json git status)
echo $CMD_RESULT | jq '.data.exit_code'
```

## Use Cases

### Development Workflows
- **Build automation**: Capture build output with integrity verification
- **Test execution**: Track test results with full provenance
- **Deployment**: Log deployment commands with timing and results

### System Administration  
- **Command auditing**: Full audit trail of executed commands
- **Output verification**: Cryptographic verification of command output
- **Performance monitoring**: Command execution duration tracking

### CI/CD Integration
- **Build provenance**: Complete build command history with digests
- **Test reporting**: Structured test output with exit codes
- **Deployment tracking**: Deployment command audit trail

For general plugin architecture and development patterns, see [../README.md](../README.md).
