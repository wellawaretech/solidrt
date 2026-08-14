---
title: Driving more than one client at once over MCP
description: With a desktop and a phone attached the whole session ran against one client while the phone sat on its initial screen, indistinguishable from a crash to the person holding it; broadcast and form-factor signals would remove the incentive to single-thread.
created: 2026-07-27
---

# Driving more than one client at once over MCP

What it looks like when you hit it: two clients are connected, the agent works
happily against one, and the other stays frozen on its first screen. To the
human holding that device it looks like a crash. Nothing in the tool surface
suggests the second client is even worth driving.

Three fixes:

- **`call_debug` with `client: "all"`** - broadcast, results keyed by client id.
  Every connected screen then moves in lockstep with the agent, which removes
  the incentive to single-thread in the first place.
- **`list_clients`: add windowSize, displayScale, orientation.** Today nothing
  signals that another client is a different form factor deserving its own
  verification pass. (Version, profile, capabilities and queries are already
  reported - this is the missing form-factor half.)
- **A scaffold/AGENTS.md paragraph.** `reload` pushes to all clients, but
  `call_debug`, `get_snapshot` and log cursors are per client, and interactive
  state does not sync. The guidance is: drive state on every client (or say
  which one you are using), and snapshot each distinct form factor before
  calling a visual change done.

Split out of a five-part round-2 agent dev-loop feedback item when okf was
restructured. Two of that item's five parts had already landed (readOnlyHint
annotations, and the mounted/orphan node counters in `get_stats`); the other
two open parts are
[mcp-interaction-perf-visibility](mcp-interaction-perf-visibility.md) and
[mcp-detached-node-bounds](mcp-detached-node-bounds.md).
