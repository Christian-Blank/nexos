# bundle plugin

A NexOS CLI plugin

## Commands

- `bundle hello [--name TEXT]` — say hello

## Implementation Notes

- This plugin implements `Plugin` with `name() = "bundle"`.
- Add/rename commands in `augment_commands()` and route in `execute()`.
- Keep outputs using the core envelope (`ctx.ok / ctx.err`) to stay machine-friendly.


## Commands

<!-- BEGIN: AUTO (id=commands) -->
- bundle build — Build a bundle (.tgz) with manifest and inputs
- bundle verify — Verify a bundle
<!-- END: AUTO -->
