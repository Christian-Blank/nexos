/newtask

You are operating inside the NEXOS repo with strict guardrails.
Goal: Implement the full plan <TASK_TITLE> <TASK_PLAN_FILE> end to end with minimal diffs, high determinism, and full observability.

Read first:
  - @/.clinerules/compiled.md
  - @/docs/PLAYBOOK.md
  - If this task references a workflow spec: open it.
  - Plan <TASK_PLAN_FILE> for full context and acceptance criteria.

Scope:
  - Primary scope label: <SCOPE_LABEL>  (examples: workflow-plugin, dev-plugin, task-plugin, observability, cline-ingest, rules-docs)
  - If SCOPE_LABEL not set, it is up to your discretion and best judgement to pick the best fit.
  - Only touch files allowed by .clinerules/scope.json for that scope. 
  - If you need to go outside scope: stop, open a Change Proposal (CP), and continue only after adding ADR ID.

Execution model:
  - Read the full plan (and its planning files) and understand the plan.
    - Understand the current implementation and context and verify the plan against the codebase.
    - If the plan or feature is already implemented in a similar way, understand first where it diverges, and implement the required changes/extensions/improvements on your discretion to match goals of the plan.
  - When running terminal commands use 'nexoscli term run -- <COMMAND>' over direct terminal commands to ensure full observability. You are free to run commands outside of nexoscli if it unblocks you or removes friction. However if you need to run commands outside nexoscli create a ticket with the blocker or friction that prevented you from doing so, so we can fix the command or process at a later point. The goal is to be most effective while continously improving and have a data driven approach, not to stifle progress or slow down without a justification.
  - Prefer running the curated workflow spec for this task:
    - If provided: `./target/debug/nexoscli workflow run --spec <SPEC_PATH> --output json`
    - Otherwise: follow the Implementation Loop below.
  - Every friction/idea: immediately create an atomic issue so we stay focused:
    `./target/debug/nexoscli issue new --title "<short>" --labels ["friction","<scope>"] --body "<why/where>" --output json`

Implementation Loop:
  1) Preflight - Do this first before anything else:
     - `cargo run -p nexoscli -- dev init --yes`
     - `cargo run -p nexoscli -- task new --title "<TASK_TITLE>"` # Creates a new task with id
     - `cargo run -p nexoscli -- task start --task <TASK_ID>` # Start task for proper context correlation in observability
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
  6) End
      - `cargo run -p nexoscli -- task end` # Finish task for proper context correlation in observability
      - `cargo run -p nexoscli -- task bundle --task <TASK_ID> --out ./<TASK_ID>-bundle.tgz` # Create a task bundle for traceability and handoff

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
  - Commit title: `feat(<scope>): <comprehensive and detailed summary> (<TASK_ID>)`
    -  If contracts changed: `feat(<scope>): <comprehensive and detailed summary> (ADR-YYYYMMDD-<slug>, <TASK_ID>)`
  - Commit message: Include full context so parsing or reading history is sufficient to get the full context from reading the history without viewing file changes.

Escalate:
  - If blocked > 10 minutes or scope fence is too tight → open CP + issue, then pause and switch.