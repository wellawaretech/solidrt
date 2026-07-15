---
type: known-limitation
title: Node snapshots depend on a frame happening; a truly idle client times out
description: get_snapshot / captureSnapshot latch a frame request but do not wake the render loop. Today the idle Tick services it anyway; if the client is idle with no ticking (true JS idle, paused/occluded window, backgrounded app), the capture is never serviced and the MCP query times out with a confusing error.
status: deferred
tags: [snapshot, capture, mcp, rendering, demand-driven]
timestamp: 2026-07-13T00:00:00Z
---

# The limitation

Both capture entry points are serviced only during a paint pass:

- JS `captureSnapshot(nodeId)` (`flux/src/plugins/gui/texture.rs`) calls
  `request_capture` then `platform.request_frame()`.
- MCP `get_snapshot` (`lattice/src/go/connection.rs` `request_snapshot`)
  calls `request_capture` then latches `frame_requested`.

`request_capture` only queues the request; alloy services it when the paint
walk next visits the node (`alloy/src/rendertree/composite.rs`
`build_recursive` -> `service_captures`), and delivers the result at the end
of `paint_phase` (`deliver_captures`). No paint, no snapshot.

The problem is that `request_frame` (`alloy/src/rendertree/platform.rs:82`)
**only latches an atomic**. It does not wake the render loop. The loop
(`alloy/src/app.rs`) sleeps on the SDL event queue and only re-checks the
latch when it naturally wakes - on input, on a submitted frame's `FrameReady`
push, or on its own idle-Tick timeout.

Right now this works purely because the idle Tick always fires: `app.rs:231`
emits an `AlloyEvent::Tick` every refresh period whenever no frame was
produced, which drives a `frame()` -> "render" event -> the demand gate
(`lattice/src/plugins/draw.rs:108`) sees `take_frame_requested() == true` and
paints. So a capture is serviced within ~one refresh period. Verified working
this way against the terminal example on 2026-07-13.

# Why it is fragile

The whole thing rests on "a frame will happen soon on its own," which is only
true while the idle Tick keeps ticking. That assumption breaks in exactly the
directions this codebase is heading or already hits:

- **True JS idle** (see the demand-driven-rendering work): the explicit goal
  is to stop the loop from ticking when nothing needs to change. The moment
  idle Ticks stop, a latch set from the MCP thread has nothing to wake the
  loop, and the capture is never serviced.
- **Paused / occluded / backgrounded client**: a minimized or backgrounded
  window (notably Android in the background) may not tick at all. Same result.
- **Cross-thread by nature**: the MCP request arrives on the dev-server
  connection thread and sets the atomic; the render loop is a different
  thread. A pure atomic write is invisible to a sleeping event loop.

When it does fail, it fails badly: the capture sits pending, no reply is ever
sent, and `handleQuery` (`packages/cli/server/control.ts:15,86`) times out
after 5s and returns a generic `{"error":"Query timed out"}`. The caller has
no way to tell "node does not exist" (which does report cleanly, via
`fail_unserviced_captures`) from "the client just is not painting."

# Fix direction

1. **Make `request_frame` actually wake the loop, not just latch.** The wake
   primitive already exists: `Context::submit` pushes a `FrameReady` user
   event via the `wake` closure registered in `app.rs`. Give
   `PlatformContext` (or the capture path) a comparable wake handle so
   requesting a frame nudges the SDL event loop the same way a submitted
   frame does. This is the general fix and makes captures (and any other
   `scheduleFrame` caller) robust to demand-driven idle. It is also a
   prerequisite for true JS idle to not silently break on-demand capture.
2. **Bound the wait and report a real reason.** Even with (1), a paused or
   backgrounded client genuinely cannot paint. `captureSnapshot` should have
   its own deadline and reject with a clear "client is not rendering (no
   frame within Nms)" rather than relying on the dev-server's generic 5s
   query timeout, and `get_snapshot` should surface that reason instead of
   "Query timed out."

# Notes

- Not to be conflated with two adjacent, already-fixed capture bugs: the
  latch-ordering race (latch the frame only after the capture is queued, see
  `request_snapshot` in connection.rs) and commit 6248b9f "Force snapshots"
  (2026-07-15), which makes the demand gate's whole-frame reuse path run a
  real paint while captures are pending - reuse skips the paint walk that
  services them. This item is only the remaining wake gap described above.

- Do not "fix" this by reintroducing an unconditional idle Tick or a poll cap
  - that fights the demand-driven-rendering design on purpose (see that
    work's note: the 8ms poll cap was removed deliberately, do not
    reintroduce). The fix is an active wake on frame request, not more
    free-running.
- Scope check before building: confirm whether any real client config today
  (desktop backgrounded window, Android background) already stops ticking, or
  whether this only bites once true JS idle lands. That decides urgency.
