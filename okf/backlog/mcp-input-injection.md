---
type: backlog-item
title: MCP input injection
description: Synthetic key and pointer events to clients, plus a snapshot-diff helper, so an agent can navigate and verify visuals without a human ferrying the app around.
status: deferred
timestamp: 2026-07-15T00:00:00Z
---

# MCP input injection

Motivation (doom renderer migration, 2026-07-15): during visual debugging the
agent's only eyes are `get_snapshot`, but its only way to change the viewpoint
is asking the human to walk the player somewhere and say "ready". Every
reload teleported the player back to the start, so reaching a bug site again
cost a human round-trip each time. With input injection the loop
"navigate -> capture -> verify" becomes fully autonomous.

Proposal: a `send_input` MCP tool (and matching dev-server control endpoint,
`packages/cli/src/commands/mcp.ts` -> `/input?client=N`) that forwards
synthetic events to a running client:

- `{ type: "key", key: "w", action: "down" | "up" | "tap" }` - injected where
  SDL key events enter the runtime, so focus rules and onKeyDown/onKeyUp
  behave exactly as for real input (same names the runtime reports, e.g.
  "Left"/"Right", not web-style).
- `{ type: "pointer", x, y, action: "down" | "up" | "move" }` for click/drag
  UIs.
- Optional `hold_ms` for keys: press, wait, release server-side, so "walk
  forward 500ms" is one call instead of two plus agent-side timing (agents
  cannot sleep precisely between tool calls).

Companion (cheap once this exists): a snapshot-diff helper - `get_snapshot`
against the previous capture of the same node (mean/max pixel delta, coarse
grid like doom's old "p" tool) - turns "does it still render the same after
my change" into one call with a numeric answer instead of eyeballing two
images.

Safety: dev-server-only surface, same trust level as `reload` (which already
pushes arbitrary code), so no new trust boundary.

Round-2 additions (app-port feedback 2026-07-17, finding 10): also
`{ type: "text", text: "g" }` events through the real TextInput path, and
timed sequences with per-event `delayMs`
(`[{text:"g"},{text:"o",delayMs:80},...]`) so realistic event rates are one
call - agent round-trips (~1s each) cannot approximate a 10-keys/sec burst.
This is the missing half of the verify loop: call_debug sets signals
directly, bypassing focus/keys/TextInput, so "verified working" never covers
the real input pipeline where latency lives. Pairs with the tracing items in
[[mcp-agent-loop-improvements]].
