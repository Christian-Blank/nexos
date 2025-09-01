# dev plugin

A NexOS CLI plugin

## Commands

- `dev hello [--name TEXT]` — say hello

## Implementation Notes

- This plugin implements `Plugin` with `name() = "dev"`.
- Add/rename commands in `augment_commands()` and route in `execute()`.
- Keep outputs using the core envelope (`ctx.ok / ctx.err`) to stay machine-friendly.

