# Flux

Flux is the JavaScript runtime underneath SolidRT: QuickJS plus a set of
capability modules, embeddable and small. It serves the same role Bun or
Deno serve elsewhere, general purpose by design - SolidRT uses it for UI,
but nothing in it is UI-specific.

Web-standard APIs (`fetch`, `Response`, `console`, timers, WebSocket) are
global; everything else is an explicit `flux:*` module:

- `flux:http` - serve HTTP and WebSockets, Bun-compatible shape.
- `flux:fs` - files and directories.
- `flux:sqlite` - SQLite.
- `flux:subprocess` - spawn processes.
- `flux:p2p` - direct peer-to-peer connections.
- `flux:wasm`, `flux:ffi`, `flux:net`, `flux:path`, `flux:process`, ...

Feature availability is by capability, not by OS: check
`Flux.capabilities` at runtime.

## Planned here

- **Reference** - one page per module, generated from the published types.
- **Examples** - generated from the repository's `flux/examples/`.

This site is built by a flux script.
