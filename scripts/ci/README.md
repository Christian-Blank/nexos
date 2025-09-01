# CI Scripts

Validation scripts for NEXOS project structure and contracts.

## validate_origin.py

Validates that required origin documents exist and exec_graph.yaml contains proper structure.

### Requirements
- Python 3.8+
- PyYAML

### Quick Start

From project root:

```bash
# Option 1: Using virtual environment (recommended)
uv venv scripts/ci/.venv
uv pip install --python scripts/ci/.venv/bin/python pyyaml
scripts/ci/.venv/bin/python scripts/ci/validate_origin.py

# Option 2: If PyYAML is already installed
python scripts/ci/validate_origin.py
```

### Output
JSON format with `passed` boolean and `errors` array:
```json
{
  "passed": true,
  "errors": []
}
```

Exit codes:
- `0`: All validations passed
- `1`: One or more validations failed

### What it validates
- Required origin files exist (north-star, project-brief, exec_graph, ADR-0001, playbook)
- exec_graph.yaml contains at least one feature and one issue node
- Node IDs follow expected format pattern
