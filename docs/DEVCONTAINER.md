# VS Code Dev Container for NEXOS

This repository includes a reproducible development environment via VS Code Dev Containers. It aims to be deterministic and aligned with the repository’s operating rules.

What’s included
- Base: Ubuntu 24.04 (devcontainers/base)
- Rust: stable toolchain pinned by rust-toolchain.toml, with rustfmt + clippy
- Python: 3.11 (satisfies scripts/ci requires-python & uv.lock)
- Node.js: v24 (via devcontainer feature) and .nvmrc=24 for local parity; Corepack enabled
- uv: Installed for reproducible Python tooling for scripts/ci
- Git/SSH: Agent forwarding and read-only ~/.ssh + ~/.gitconfig mounts
- VS Code extensions in container:
  - rust-lang.rust-analyzer
  - tamasfe.even-better-toml
  - vadimcn.vscode-lldb
  - serayuzgur.crates
  - ms-python.python
  - EditorConfig.EditorConfig
  - DavidAnson.vscode-markdownlint
  - Cline.cline (Cline VS Code extension)

Files
- .devcontainer/devcontainer.json
- .nvmrc (contains: 24)

Using the Dev Container
1) Open this folder in VS Code
2) Run “Dev Containers: Reopen in Container”
3) Wait for postCreate to finish (toolchains, uv, corepack, etc.)

Pre-flight commands (run inside the container)
These commands follow the project’s .clinerules/compiled.md guidance.

- Build and test:
  cargo build --workspace
  cargo test  --workspace

- Initialize dev environment:
  cargo run -p nexoscli-bin -- dev init --yes

- Health checks (required checks: plugins, graph):
  cargo run -p nexoscli-bin -- dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph

- Build docs & tool specs (no hand-editing generated outputs):
  cargo run -p nexoscli-bin -- dev docs build

- Optional: telemetry report
  cargo run -p nexoscli-bin -- dev telemetry report --since 24h

Node & package managers
- Node 24 is available by default via the devcontainer feature.
- .nvmrc is set to 24 to keep local + container Node versions in sync.
- Corepack is enabled to support Yarn/Pnpm without global installers.

Python & uv
- Python 3.11 is installed.
- uv is installed to support deterministic environments for scripts/ci.
- Example (if you need to run a CI helper locally):
  uv pip install pyyaml
  uv run python scripts/ci/validate_origin.py

Notes
- Debugging is enabled via SYS_PTRACE and LLDB (vadimcn.vscode-lldb).
- SSH agent forwarding and mounts let you use git@github.com remotes.
- All changes to the environment are isolated to the container to support reproducibility.

If you need Node.js-based tooling for front-end (e.g., Next.js + TypeScript), this container is already ready (Node 24 + Corepack). If additional tools are required later, prefer capturing them via package manager config rather than global installs to preserve determinism.
