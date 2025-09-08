# graph plugin

A NexOS CLI plugin

## Commands

- `graph hello [--name TEXT]` — say hello

## Implementation Notes

- This plugin implements `Plugin` with `name() = "graph"`.
- Add/rename commands in `augment_commands()` and route in `execute()`.
- Keep outputs using the core envelope (`ctx.ok / ctx.err`) to stay machine-friendly.


## Commands

<!-- BEGIN: AUTO (id=commands) -->
- graph check — Validate docs/exec_graph.yaml (acyclic, unique ids)
- graph impact — List nodes impacted (downstream) by the given id
<!-- END: AUTO -->
