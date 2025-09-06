# Use bash for more capable recipes
set shell := ["bash", "-cu"]

# Default help
default:
  @echo "Available recipes:"
  @echo "  just regen-schemas       # Export schemas to contracts/schemas"
  @echo "  just verify-schemas      # Export to temp and diff against contracts/schemas (drift check)"
  @echo "  just run-example WF=...  # Run an example workflow spec"
  @echo "  just bundle-task TASK=...# Build a task bundle (evidence archive)"

# Export contract schemas (schemars) to frozen snapshots
regen-schemas:
  cargo run -q -p nexoscli -- dev schemas export --out contracts/schemas

# Verify schema snapshots are drift-free (fails on diff)
verify-schemas:
  tmp="$(mktemp -d)"; \
  cargo run -q -p nexoscli -- dev schemas export --out "$tmp"; \
  diff -ru "$tmp" contracts/schemas --exclude README.md

# Run an example workflow (defaults to timer demo)
run-example WF="examples/workflows/WF-timer-demo.yaml":
  cargo run -q -p nexoscli -- workflow run --spec {{WF}} --output json

# Build a task bundle (archives evidence, useful for provenance)
bundle-task TASK?="current":
  cargo run -q -p nexoscli -- task bundle --task {{TASK}} --out bundle.tgz
