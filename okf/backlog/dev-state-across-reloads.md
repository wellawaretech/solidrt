---
type: backlog-item
title: Dev-state KV that survives hot reloads
status: deferred
timestamp: 2026-07-15T00:00:00Z
---

# Dev-state KV that survives hot reloads

Motivation (doom, 2026-07-15): every `reload` restarts the bundle from
scratch, so the player teleports back to the start position. During a long
visual-debugging session (dozens of reloads) re-walking to the bug site
dominated iteration time - one bug needed noclip added just to make the spot
reachable again.

Proposal: a per-client key-value store owned by the flux host (not the JS
context), surviving bundle reloads but not client restarts:

```ts
import { devState } from "flux:dev"
// on change (or on a timer):
devState.set("pose", { x, y, angle, sec })
// on startup:
let pose = devState.get("pose") ?? defaultStart
```

- Values JSON-serialized at the boundary; host keeps the map per client.
- Dev-only: in release builds `get` always returns undefined (or the module
  is absent), so apps can leave the calls in.
- Explicit opt-in beats trying to snapshot/restore arbitrary JS state -
  the app knows which few values ("pose", "selected tab", "scroll") make a
  reload seamless.
- Belongs in the same `flux:dev` module as registerDebug (see
  mcp-debug-commands.md); an MCP `get_dev_state`/`set_dev_state` passthrough
  falls out for free and lets an agent teleport the app into a known state
  before capturing.
