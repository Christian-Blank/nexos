# Cline Checklist (tick every box)

- [ ] Ran `nexoscli dev init --yes`
- [ ] `nexoscli dev doctor --ci` passed with required checks (plugins, graph)
- [ ] Chosen scope label; edits limited to `.clinerules/scope.json`
- [ ] Started a task (`task start`) so events include {task_id, session_id}
- [ ] Used generator for any new plugin or structure
- [ ] Added/updated **E2E** tests only (assert JSON envelopes)
- [ ] `cargo test --workspace` passed locally
- [ ] `nexoscli dev plugins check` OK
- [ ] `nexoscli dev docs build` ran; did not hand‑edit generated docs
- [ ] If contracts changed: ADR created & commit title contains `ADR-...`
- [ ] `nexoscli dev telemetry report --since 24h` inspected
- [ ] `nexoscli task end` and `nexoscli task bundle --out ./bundle.tgz` created