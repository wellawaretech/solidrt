---
type: backlog-item
title: App-registered debug commands callable via MCP
status: done
timestamp: 2026-07-15T00:00:00Z
---

Implemented 2026-07-15, with one deviation from the sketch below: the module
is the existing `srt:dev` (lattice), not a new `flux:dev` - the dev-server
connection is lattice/go's domain and `srt:dev` already had the right
availability story (registered in every build, no-op outside go). registerDebug
stores Persistent functions in a DebugRegistry ctx userdata (resets on hot
reload as proposed); MCP list_debug/call_debug -> /__control__/debug (GET
list, POST call with JSON body args) -> debug_list/debug_call query kinds,
called synchronously on the JS thread. Return values JSON-stringified
(undefined -> null); thrown errors become error replies with bundle positions
remapped to .tsx. Async (promise-returning) commands not supported yet.
Verified end-to-end against doom (args in/out, throw path).

# App-registered debug commands (MCP `call_debug`)

Motivation (doom, 2026-07-15): agents debugging a running app keep needing
answers only the app has - "where is the player, which sector, which door is
open, what pose produced this frame". The workaround was binding debug
actions to keys ("p" parity diff, "o" door toggle, "n" noclip) and reading
results back via `get_logs`. That pollutes the app's real input handling,
needs the human to press the key, and returns data as log strings.

Proposal: a tiny registry in the runtime, e.g. `flux:dev`:

```ts
import { registerDebug } from "flux:dev"
registerDebug("pose", () => ({ x, y, angle, sec }))
registerDebug("door", (args) => toggleDoor(args.sector))
```

plus MCP tools `list_debug(client)` and `call_debug(client, name, args?)`
(dev-server control endpoint -> client -> JSON-serialized return value).
Calls run on the JS thread like any event; return values must be
JSON-serializable.

Notes:
- No-op / stripped in release builds; this is a dev-server capability like
  reload.
- Replaces the debug-keys pattern entirely: doom's noclip/door-toggle/pose
  would move here, and combined with input injection (see
  mcp-input-injection.md) an agent can steer, query, and capture without a
  human in the loop.
- Registry should survive hot reload naturally (re-registration on module
  init is fine); duplicate names replace.
