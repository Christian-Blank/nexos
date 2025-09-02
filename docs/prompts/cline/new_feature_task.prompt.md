/newtask

You are operating inside the NEXOS repo with strict guardrails.
Goal: Implement the plan for <TASK_TITLE> with minimal diffs, high determinism, and full observability.
Plan: Read the following file(s) for the task plan: <TASK_PLAN>
Important: If TASK_TITLE is not set, determine yourself based on context and best judgement.

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
  - Prefer running the curated workflow spec for this task:
    - If provided: `./target/debug/nexoscli workflow run --spec <SPEC_PATH> --output json`
    - Otherwise: follow the Implementation Loop below.
  - Every friction/idea: immediately create an atomic issue so we stay focused:
    `./target/debug/nexoscli issue new --title "<short>" --labels ["friction","<scope>"] --body "<why/where>" --output json`

Implementation Loop:
  1) Preflight
     - `nexoscli dev init --yes`
     - `nexoscli dev doctor --ci --checks graph,plugins,config,events,tools --require plugins,graph`
     - `nexoscli dev plugins check`
  2) Make the smallest change set to satisfy acceptance. Generators before hand-rolled.
  3) Validate
     - `cargo build --workspace`
     - `cargo test --workspace`
     - `nexoscli dev docs build`
     - `nexoscli dev telemetry report --since 24h`
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
  - `nexoscli dev doctor --ci` green
  - Docs updated (READMEs short; CLI reference regenerated)
  - Cline rules updated if surface changed
  - Task bundle produced: `nexoscli task bundle --out ./bundle.tgz`
  - Conventional commit with Task ID (and ADR ID if any) and comprehensive and detailed commit message to get the full context from reading the history without needing to view the changes

Commit format:
  - `feat(<scope>): <comprehensive and detailed summary> (<TASK_ID>)`
  - If contracts changed: `feat(<scope>): <comprehensive and detailed summary> (ADR-YYYYMMDD-<slug>, <TASK_ID>)`

Escalate:
  - If blocked > 10 minutes or scope fence is too tight → open CP + issue, then pause and switch.