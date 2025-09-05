# Policy (Human View)

This repository uses a strict contracts‑first approach. Agents and humans must:

- Keep changes **scoped** (see `.clinerules/scope.json`), or open a change proposal.
- Prefer **generators** (`nexoscli dev plugin new`) over hand‑rolled files.
- Emit **observable** outputs via `ctx.ok/ctx.err` so events correlate with tasks.
- Maintain **snapshots** of CLI/Tool contracts. Breaking changes require ADR.
- Write **E2E tests** that act via the CLI and assert JSON envelopes.
- Use **task** lifecycle so work is traceable and reproducible (`task bundle`).

Examples:
- Plan start: `nexoscli dev plans start --output json`
- Plan finalize: `nexoscli dev plans finalize --output json`
- New plugin: `nexoscli dev plugin new --name demo --register --yes`
- Health: `nexoscli dev doctor --ci --checks graph,plugins,events`
- Contracts: `nexoscli dev docs build` then commit if no drift.
- TracePack/NEXBags: `nexoscli replay bag build --run-id RUN-... --out ./bag.tgz` then `nexoscli replay bag verify --bag ./bag.tgz` (see docs/TRACEPACK.md)
