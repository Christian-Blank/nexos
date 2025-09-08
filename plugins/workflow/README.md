# workflow plugin

A NexOS CLI plugin

## Commands

- `workflow hello [--name TEXT]` — say hello

## Implementation Notes

- This plugin implements `Plugin` with `name() = "workflow"`.
- Add/rename commands in `augment_commands()` and route in `execute()`.
- Keep outputs using the core envelope (`ctx.ok / ctx.err`) to stay machine-friendly.


## Commands

<!-- BEGIN: AUTO (id=commands) -->
- workflow init — Create a workflow spec template
- workflow list — List workflows using search-attribute filters
- workflow plan — Plan which steps would run (idempotency-aware)
- workflow query — Query workflow runtime state
- workflow run — Run a workflow spec sequentially
- workflow signal — Send a signal to a running workflow
- workflow status — Show effects/history for a spec id (plus visibility if present)
- workflow upsert-sa — Upsert search attributes for a workflow
- workflow validate — Validate a workflow spec (schema + invariants)
<!-- END: AUTO -->
