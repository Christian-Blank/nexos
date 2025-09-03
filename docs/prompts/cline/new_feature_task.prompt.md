/newtask

You are operating inside the NEXOS repo with strict guardrails.
Goal: Implement the full plan <TASK_TITLE> <TASK_PLAN_FILE> end to end with minimal diffs, high determinism, and full observability.

Read first:
  - @/.clinerules/compiled.md
  - @/docs/PLAYBOOK.md
  - If this task references a workflow spec: open it.

Scope:
  - Primary scope label: <SCOPE_LABEL>  (examples: workflow-plugin, dev-plugin, task-plugin, observability, cline-ingest, rules-docs)
  - If SCOPE_LABEL not set, it is up to your discretion and best judgement to pick the best fit.
  - Only touch files allowed by .clinerules/scope.json for that scope. 
  - If you need to go outside scope: stop, open a Change Proposal (CP), and continue only after adding ADR ID.

Execution model:
  - Understand the current implementation and context and verify the plan against the codebase.
    - If the plan or feature is already implemented in a similar way, understand first where it diverges, and implement the required changes/extensions/improvements on your discretion to match goals of the plan.
  - Prefer running the curated workflow spec for this task:
    - If provided: `./target/debug/nexoscli workflow run --spec <SPEC_PATH> --output json`
    - Otherwise: follow the Implementation Loop below.
  - Every friction/idea: immediately create an atomic issue so we stay focused:
    `./target/debug/nexoscli issue new --title "<short>" --labels ["friction","<scope>"] --body "<why/where>" --output json`

Implementation Loop:
  1) Preflight
     - `cargo run -p nexoscli -- dev init --yes`
     - `cargo run -p nexoscli -- dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph`
     - `cargo run -p nexoscli -- dev plugins check`
  2) Make the smallest change set to satisfy acceptance. Generators before hand-rolled.
  3) Validate
     - `cargo build --workspace`
     - `cargo test --workspace`
     - `cargo run -p nexoscli -- dev docs build`
     - `cargo run -p nexoscli -- dev telemetry report --since 24h`
  4) If contracts changed (CLI surface/help/schema): write ADR, update snapshots in the dedicated PR.
  5) Repeat until Done.

Definition of Ready (must be true to start):
  - Problem ≤ 3 sentences
  - DRI set
  - Scope label chosen
  - ≤ 8 binary acceptance criteria
  - Exec graph node exists or will be added in this task

Definition of Done (must be true to finish):
  - All acceptance pass (E2E first)
  - JSON envelopes stable; events emitted; spans present if applicable
  - `cargo test --workspace` green
  - `cargo run -p nexoscli -- dev doctor --ci` green
  - All relevant docs updated (READMEs short; CLI reference regenerated)
  - Checked and updated cline rules if surface or anything in development process changed
  - Task bundle produced: `cargo run -p nexoscli -- task bundle --out ./bundle.tgz`
  - Conventional commit with Task ID (and ADR ID if any) and comprehensive and detailed commit message to get the full context from reading the history without needing to view the changes

Commit format:
  - `feat(<scope>): <comprehensive and detailed summary> (<TASK_ID>)`
  - If contracts changed: `feat(<scope>): <comprehensive and detailed summary> (ADR-YYYYMMDD-<slug>, <TASK_ID>)`
  - Commit message: Include full context so parsing or reading history is sufficient to get the full context from reading the history without viewing file changes.

Escalate:
  - If blocked > 10 minutes or scope fence is too tight → open CP + issue, then pause and switch.