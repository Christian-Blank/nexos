# Start New Task — Next → Current (v1, CLI-native)

Purpose: Move the top plan from docs/project/next/ to current/, create & start a task, write task_id into the plan front‑matter, and begin implementation with guardrails.
Input: none (selects first PLAN-*.md under docs/project/next/).
Output: one implementation commit (divider happens in the Finalize workflow).

---

## 0) Pre‑flight (required)

- First read this workflow and all referenced workflow, and create a checklist for all required steps in sequence of the full workflow. As you execute each step make sure to check off each step in the checklist after you sucessfully executed it. If helpful add clear triggers or information to your checklist for each step when they are finished and should be checked off, to not forget to check off each step one by one. 

- Prepare worktree (This enables parallel work in the same git repository on the same machine. [Official documentation for trouble shooting or deeper understanding of worktrees if required](https://git-scm.com/docs/git-worktree))
  * Check that repo is on main and is up to date 'git checkout main && git pull'
  * Set `git config --global push.autoSetupRemote true`
  * Check if branch for task exists `git branch`
    If no branch exists yet:
      - Create a new worktree directory with a new branch `git worktree add ../nexos_task-<TASK_NO> -b task/<TASK_NO>-<TASK_NAME>`, where "nexos_task-<TASK_NO>" is the folder name, and "task/<TASK_NO>-<TASK_NAME>" is the branch name.
    If branch exists:
      - Check if worktree dir already exists: `git worktree list`
      If worktree dir exists, switch to worktreedir, otherwise create worktree directory with existing branch: `git worktree add ../nexos_task-<TASK_NO> task/<TASK_NO>-<TASK_NAME>`
  * Finally push new branch to remote `git push -u origin task/<TASK_NO>-<TASK_NAME>`
  Worktree preparation done.

- Read:
  - @/.clinerules/compiled.md
  - @/docs/PLAYBOOK.md

- Run:
  cargo run -p nexoscli dev init --yes
  cargo run -p nexoscli dev doctor --ci --checks="graph,plugins,config,events,tools" --require="plugins,graph"

- Choose a scope label (e.g., workflow-plugin, dev-plugin, task-plugin, observability, cline-ingest). Respect .clinerules/scope.json. If you must go beyond scope: stop, open CP/ADR.

---

## 1) Start plan (creates+starts task, updates plan, moves next → current)

- Start using the CLI-native flow (front‑matter updated in Rust; no Python/shell templating):

  cargo run -p nexoscli dev plans start --output json

What happens:
- Picks first docs/project/next/PLAN-*.md (or pass --plan PATH), first as in plan ascending by number.
- Creates task and starts it.
- Updates front‑matter: status=in-progress, plan_id, slug, task_id, started_at, source=next.
- Moves the file to docs/project/current/.

---

## 2) Implementation loop (minimal diffs, deterministic)

- Read the plan fully (context + acceptance criteria).
- Prefer generators over hand‑rolled; keep diffs ≤ ~300 LOC or ≤ 6 files.
- For shell commands with observability:
  ./target/debug/nexoscli term run -- <your command>

- Validate frequently:
  cargo build --workspace
  cargo test  --workspace
  cargo run -p nexoscli dev docs build
  cargo run -p nexoscli dev telemetry report --since 24h

- If public contracts changed: write/attach ADR (docs/ADRs/ADR-YYYYMMDD-*.md) and update snapshots in the dedicated PR.

---

## 3) Implementation commit (not the divider)

Make sure the context is clear from the commit message alone.

git add --all
git commit -m "feat(<scope>): implement PLAN-<ID> <slug> (<TASK_ID>)
- Summary of what/why (plan link)
- Evidence (E2E tests / envelopes)
- Notes (if ADR or CP)
git push
"

Stop here. The divider & plan finalization happens in the Finalize workflow.

---

## 4) Human Review

Stop and ask the human for review and feedback, point out any specifics that need attention, corner cases, and information that helps the human for an effective review.

## 5) Finalize Task

Once the human approved the review, run the finalizing workflow and continue the steps in the [Finalize Task Workflow](./finalize_task.workflow.md).

## Troubleshooting

- Error starting a new task due to an already active one:

  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
  Running `target/debug/nexoscli task start --task TASK-20250905-build`
  Error: Other: task 'TASK-20250905-build' is active; pause it before starting/resuming another task

  Resolution:
  - Pause current active task:
    cargo run -p nexoscli task pause
  - Retry start:
    cargo run -p nexoscli dev plans start --output json
  - If you must end the previous task first:
    cargo run -p nexoscli task end --task TASK-20250905-build || true
    cargo run -p nexoscli dev plans start --output json
