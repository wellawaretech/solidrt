---
title: MCP input injection
description: Synthetic key and pointer events to clients, plus a snapshot-diff helper, so an agent can navigate and verify visuals without a human ferrying the app around.
created: 2026-07-15
completed: 2026-08-10
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
images. (Split out to [[snapshot-diff-helper]] at implementation time: not
actually cheap, the CLI has no PNG codec so the runtime must retain raw
captures to diff against.)

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

## Implementation (2026-08-07)

Landed as the `send_input` MCP tool, the stage-4 follow-up to
[[mcp-verification-surface]]. Decisions and traps:

- Seam: `DevFlags::input_tx`, a clone of the UI thread's batch-loop channel
  (`lattice/src/lib.rs`, next to the bridge thread), plus
  `DevFlags::resampler` since alloy took over the resampler feeding:
  injected moves are consumed into the resampler at the send site
  (producer-side, mirroring the alloy pump; they never travel as events)
  while downs/ups/wheels ride the channel. Injected events still take the
  ENTIRE real pipeline - resampler frame slots, InputState bookkeeping
  (hover refresh), capture forwarding, PointerRouter hit testing and drag
  capture, focus + text-session activation. Deliberately no
  frame_requested latch: real input does not latch either; the app's
  handlers request whatever frames their reactions need.
- Vocabulary (wire JSON, parsed in `connection.rs::parse_input_events`):
  `key` (down/up/tap, `holdMs` on tap, modifier booleans; W3C `key` names
  as the runtime reports them, `code` derived by `alloy::w3c_code_for_key`
  - US-layout positions, "Unidentified" for shifted punctuation);
  `pointer` (down/up/move/tap; pointerType mouse default or touch; button
  0/1/2; fixed synthetic pointer id `1 << 60`); `wheel`; `text`. Per-event
  `delayMs`. Caps: 5000 ms per delay/hold, 30 s per sequence, 200 events.
  Whole-batch validation: any bad event rejects everything before a single
  send. `hold_ms` from the proposal became `holdMs` (API camelCase); "tap"
  rather than "click" because the runtime has no click event.
- Sequences run on a spawned tokio task on the connection runtime (a hold
  never blocks the connection loop); the reply `{delivered: N}` follows the
  last event, and the dev server stretches its 5 s query timeout by the
  sequence's own delays (`handleQuery` gained a per-call timeout).
- Traps: a synthetic MOUSE pointer persists in InputState and keeps
  hovering at its last position (real-cursor semantics; use touch for
  hover-free gestures). Move bursts without delayMs coalesce to the newest
  per frame slot in the resampler. Text lands only with a focused `onTextInput` node and an
  active session - pointer-tap the field first. Injected keys are recorded
  by `--capture` exactly like real ones. `ScriptEvent` stays keyboard-only;
  extending it with pointer variants is the designed growth path if
  record/playback should share this vocabulary.
- The snapshot-diff companion did NOT ship; split to
  [[snapshot-diff-helper]].
