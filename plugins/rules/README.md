# Rules Plugin — Development Rules Management

Rules management plugin for nexoscli.
Provides initialization, linting, and export capabilities for development rules with YAML front-matter validation and audience-specific exports.

---

## Commands

### `rules init`
Initialize the rules system with template structure and core rules.

```bash
nexoscli rules init                                     # Initialize rules system
```

Creates:
- `rules/templates/rule.md` - Rule template with YAML front-matter
- `rules/RULE-use-term-run.md` - Core rule for command execution
- `rules/RULE-post-effort-rule-sweep.md` - Rule sweep workflow rule
- `rules/RULE-cline-import.md` - Cline integration rule

**JSON Output:**
```json
{
  "data": {
    "message": "Rules system initialized successfully",
    "files_created": [
      "rules/templates/rule.md",
      "rules/RULE-use-term-run.md", 
      "rules/RULE-post-effort-rule-sweep.md",
      "rules/RULE-cline-import.md"
    ]
  },
  "errors": [],
  "meta": { "run_id": "uuid", "duration_ms": 15 }
}
```

### `rules lint`
Validate rule files for proper YAML front-matter structure.

```bash
nexoscli rules lint                                     # Lint all rule files
nexoscli rules lint --strict                           # Strict validation mode
```

**JSON Output:**
```json
{
  "data": {
    "total_rules": 12,
    "valid_rules": 11,
    "invalid_rules": 1,
    "validation_errors": [
      {
        "file": "rules/RULE-invalid-date.md",
        "error": "Invalid RFC3339 timestamp in last_reviewed field"
      }
    ]
  },
  "errors": [],
  "meta": { "run_id": "uuid", "duration_ms": 45 }
}
```

### `rules export`
Export rules to audience-specific markdown bundles.

```bash
nexoscli rules export                                   # Export for all audiences
nexoscli rules export --audience human                 # Export for human audience only
nexoscli rules export --audience agent                 # Export for AI agent audience only
nexoscli rules export --audience cline                 # Export for Cline assistant only
```

**JSON Output:**
```json
{
  "data": {
    "exports_created": [
      "rules-human.md",
      "rules-agent.md", 
      "rules-cline.md"
    ],
    "total_rules_exported": 8,
    "rules_by_audience": {
      "human": 5,
      "agent": 6,
      "cline": 3
    }
  },
  "errors": [],
  "meta": { "run_id": "uuid", "duration_ms": 32 }
}
```

## Data Model

### Rule Front-Matter Structure
```yaml
---
id: RULE-example-rule
title: "Example Development Rule"
audience: [human, agent]
scope: development
status: active
version: 1.0
last_reviewed: "2024-12-01T10:00:00Z"
principle: "Clear example of rule structure"
owner: development-team
---
```

### Front-Matter Fields
```rust
pub struct RuleFront {
    pub id: String,                    // Unique rule identifier (RULE-*)
    pub title: String,                 // Human-readable rule title
    pub audience: Vec<String>,         // Target audiences: human, agent, cline
    pub scope: String,                 // Rule scope (development, deployment, etc.)
    pub status: String,                // Rule status (active, draft, deprecated)
    pub version: String,               // Rule version (semver recommended)
    pub last_reviewed: String,         // ISO 8601 timestamp of last review
    pub principle: String,             // Core principle or rationale
    pub owner: String,                 // Rule owner/maintainer
}
```

### Export Result Structure
```rust
pub struct ExportResult {
    pub audience: String,              // Target audience for export
    pub file_path: String,             // Generated export file path
    pub rule_count: usize,             // Number of rules included
    pub total_size: usize,             // Total export file size in bytes
}
```

## Rule Template

### Standard Rule Template
```markdown
---
id: RULE-your-rule-name
title: "Your Rule Title"
audience: [human, agent, cline]
scope: development
status: active
version: 1.0
last_reviewed: "2024-12-01T10:00:00Z"
principle: "Brief principle or rationale"
owner: your-team
---

# Your Rule Title

## Context
Brief context explaining when this rule applies.

## Rule
Clear, actionable rule statement.

## Rationale
Explanation of why this rule exists and its benefits.

## Examples
### ✅ Good
```
// Good example
```

### ❌ Bad  
```
// Bad example
```

## Enforcement
How this rule is enforced (manual review, automated checks, etc.).

## Related Rules
- RULE-related-rule-1
- RULE-related-rule-2
```

## Validation Rules

### YAML Front-Matter Validation
1. **Required Fields**: All fields in RuleFront struct must be present
2. **ID Format**: Must start with "RULE-" prefix
3. **Audience Values**: Must be one of: `human`, `agent`, `cline`
4. **RFC3339 Timestamps**: `last_reviewed` must be valid RFC3339 format
5. **Non-Empty Values**: All string fields must be non-empty

### File Structure Validation
- **Extension**: Rule files must have `.md` extension
- **Front-Matter**: Must have valid YAML front-matter block
- **Content**: Must have content after front-matter

### Cross-Reference Validation
- **Unique IDs**: Rule IDs must be unique across all rules
- **Valid References**: References to other rules must exist

## Audience System

### Audience Types
- **`human`**: Rules for human developers and reviewers
- **`agent`**: Rules for AI agents and automated systems  
- **`cline`**: Rules specifically for Cline AI assistant

### Audience-Specific Exports
Each audience gets a tailored export containing only relevant rules:

```bash
# Human-focused export (code review, development practices)
rules-human.md

# Agent-focused export (automated systems, CI/CD integration)  
rules-agent.md

# Cline-focused export (AI assistant specific guidelines)
rules-cline.md
```

### Multi-Audience Rules
Rules can target multiple audiences:
```yaml
audience: [human, agent]  # Rule applies to both humans and agents
audience: [cline]         # Rule applies only to Cline assistant
```

## CLI Integration

### Exit Codes
- **0**: Success (all rules valid, exports created)
- **1**: Validation errors found during lint
- **2**: File system errors (permissions, missing files)
- **3**: Invalid command arguments

### Argument Handling
```bash
# Initialize rules system
nexoscli rules init

# Validate all rules
nexoscli rules lint

# Export for specific audience
nexoscli rules export --audience human

# Strict validation mode
nexoscli rules lint --strict
```

## Doctor Integration

### Health Checks
The rules plugin integrates with `dev doctor --ci` for automated validation:

```bash
# Standard health check (soft-fail by default)
nexoscli dev doctor --ci

# Enforce rules validation (fail on rule errors)
nexoscli dev doctor --ci --require rules
```

### Doctor Check Results
```json
{
  "check": "rules",
  "status": "pass",
  "message": "All 12 rules validated successfully",
  "details": {
    "total_rules": 12,
    "valid_rules": 12,
    "export_success": true
  }
}
```

## AI Integration

### Available Tools
1. **`rules_init`** - Initialize rules system
2. **`rules_lint`** - Validate rule files  
3. **`rules_export`** - Export audience-specific bundles

### JSON Schemas
```rust
// rules_init schema
{
  "type": "object", 
  "properties": {},
  "description": "Initialize rules system with templates and core rules"
}

// rules_lint schema
{
  "type": "object",
  "properties": {
    "strict": {
      "type": "boolean",
      "description": "Enable strict validation mode"
    }
  }
}

// rules_export schema
{
  "type": "object", 
  "properties": {
    "audience": {
      "type": "string",
      "enum": ["human", "agent", "cline"],
      "description": "Target audience for export"
    }
  }
}
```

### Agent Usage Examples
```bash
# AI can initialize rules system
nexoscli rules init --output json

# AI can validate rules and get structured feedback
nexoscli rules lint --output json | jq '.data.validation_errors'

# AI can export rules for specific audiences
nexoscli rules export --audience agent --output json
```

## Core Rules

### RULE-use-term-run
- **Purpose**: Mandate use of `nexoscli term run` for command execution
- **Audience**: `[human, agent, cline]`
- **Rationale**: Ensures full provenance tracking and observability

### RULE-post-effort-rule-sweep  
- **Purpose**: Require rule review after significant development efforts
- **Audience**: `[human, agent]`
- **Rationale**: Keeps rules current and identifies new rule needs

### RULE-cline-import
- **Purpose**: Import Cline context before development work
- **Audience**: `[cline]`  
- **Rationale**: Ensures AI assistant has full project context

## Implementation Details

### File Discovery
```rust
fn find_rule_files(rules_dir: &Path) -> Result<Vec<PathBuf>> {
    WalkDir::new(rules_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension() == Some(OsStr::new("md")))
        .filter(|e| e.file_name().to_string_lossy().starts_with("RULE-"))
        .map(|e| e.path().to_path_buf())
        .collect()
}
```

### YAML Parsing
```rust
fn parse_front_matter(content: &str) -> Result<RuleFront> {
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return Err(anyhow!("No front-matter found"));
    }
    
    let yaml_content = parts[1].trim();
    let rule_front: RuleFront = serde_yaml::from_str(yaml_content)?;
    validate_front_matter(&rule_front)?;
    Ok(rule_front)
}
```

### RFC3339 Validation
```rust
fn validate_rfc3339(timestamp: &str) -> Result<()> {
    let re = Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$")?;
    if !re.is_match(timestamp) {
        return Err(anyhow!("Invalid RFC3339 timestamp: {}", timestamp));
    }
    Ok(())
}
```

### Export Generation
```rust
fn export_for_audience(rules: &[Rule], audience: &str) -> Result<String> {
    let filtered_rules: Vec<_> = rules
        .iter()
        .filter(|r| r.front_matter.audience.contains(&audience.to_string()))
        .collect();
        
    let mut export = format!("# Rules Export - {} Audience\n\n", audience);
    export.push_str(&format!("Generated: {}\n", Utc::now().to_rfc3339()));
    export.push_str(&format!("Total Rules: {}\n\n", filtered_rules.len()));
    
    for rule in filtered_rules {
        export.push_str(&rule.content);
        export.push_str("\n---\n\n");
    }
    
    Ok(export)
}
```

## Error Handling

### Validation Errors
- **Missing Front-Matter**: Rules without YAML front-matter
- **Invalid YAML**: Malformed YAML syntax
- **Missing Required Fields**: Incomplete front-matter structure
- **Invalid Values**: Incorrect audience values, malformed timestamps
- **Duplicate IDs**: Multiple rules with same ID

### File System Errors
- **Permission Denied**: Insufficient permissions for file operations
- **Missing Directory**: Rules directory doesn't exist
- **Write Failures**: Cannot create export files

### Recovery Strategies
- **Partial Success**: Continue processing valid rules even if some fail
- **Detailed Reporting**: Provide specific error messages for each failure
- **Graceful Degradation**: Export what's available even with validation errors

## Performance & Observability

### Duration Tracking
- **Rule Discovery**: Time to find all rule files
- **Validation Duration**: Time to lint all rules
- **Export Generation**: Time to create audience exports

### Memory Management
- **Streaming Processing**: Process rules individually to minimize memory usage
- **Efficient Parsing**: Parse YAML only once per rule
- **Export Batching**: Generate exports incrementally

### Event Logging
All rules operations generate structured events:
```json
{
  "plugin": "rules",
  "command": "lint", 
  "attrs": {
    "total_rules": 12,
    "valid_rules": 11,
    "validation_errors": 1
  },
  "content_hash": "sha256:...",    // Hash of validation results
  "args_hash": "sha256:...",       // Hash of CLI arguments  
  "dur_ms": 45                     // Operation duration
}
```

## Testing

### Unit Tests
- YAML front-matter parsing and validation
- RFC3339 timestamp validation
- Audience filtering logic
- Export generation accuracy

### Integration Tests
- End-to-end rule initialization
- Full validation workflows
- Multi-audience export generation
- Doctor integration functionality

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_front_matter_validation() {
        // Test YAML parsing and field validation
    }

    #[test]
    fn test_audience_filtering() {
        // Verify audience-specific exports
    }

    #[test] 
    fn test_rfc3339_validation() {
        // Test timestamp format validation
    }
}
```

---

## Quick Examples

```bash
# Initialize rules system
nexoscli rules init

# Validate all rules
nexoscli rules lint --output json

# Export for humans only
nexoscli rules export --audience human

# Check rules in CI pipeline
nexoscli dev doctor --ci --require rules

# View rule validation details
nexoscli rules lint | jq '.data.validation_errors'
```

## Use Cases

### Development Workflow
- **Rule Creation**: Initialize new rules with proper structure
- **Rule Review**: Validate rules during development
- **Documentation**: Generate audience-specific rule documentation

### CI/CD Integration  
- **Quality Gates**: Fail builds on rule validation errors
- **Documentation Updates**: Auto-generate rule exports in CI
- **Compliance Checking**: Ensure rules are up-to-date and valid

### Team Collaboration
- **Audience Targeting**: Different rules for humans vs AI systems
- **Rule Maintenance**: Track rule ownership and review cycles
- **Process Documentation**: Embed rules in development workflows

For general plugin architecture and development patterns, see [../README.md](../README.md).

## Commands

<!-- BEGIN: AUTO (id=commands) -->
- rules export — Export audience-specific bundle
- rules init — Create minimal rules pack
- rules lint — Validate rules/*.md front-matter
<!-- END: AUTO -->
