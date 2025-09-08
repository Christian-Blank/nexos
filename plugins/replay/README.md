# replay plugin

A NexOS CLI plugin

## Commands

- `replay hello [--name TEXT]` — say hello

## Implementation Notes

- This plugin implements `Plugin` with `name() = "replay"`.
- Add/rename commands in `augment_commands()` and route in `execute()`.
- Keep outputs using the core envelope (`ctx.ok / ctx.err`) to stay machine-friendly.


## Commands

<!-- BEGIN: AUTO (id=commands) -->
- replay annotate add — Add an annotation
- replay annotate list — List annotations
- replay bag build — Build a NEXBag (TracePack v0.2)
- replay bag cat — Stream a topic from the bag
- replay bag info — Show bag summary and counts
- replay bag verify — Verify a TracePack/NEXBag integrity
- replay export — Export CSV artifacts from a bag
- replay extract — Export a TracePack for a run (events + patches + manifest)
- replay fileops derive — Derive fileops.ndjson from bag patches
- replay normalize — Normalize events to v0.3
- replay plan — Plan steps for a run
- replay run — Apply patches for a run (dry-run by default)
- replay schema events — Print events schema
- replay schema get — Get schema JSON
- replay schema list — List available schemas
- replay validate — Validate bag artifacts against schemas
- replay verify — Verify a TracePack (dir or .tgz) and explain issues
<!-- END: AUTO -->
