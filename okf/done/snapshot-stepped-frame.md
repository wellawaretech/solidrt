---
title: Snapshot of a stepped frame as it was drawn
description: A snapshot captured a paint it requested itself, after the pending microtasks, so a glitch confined to one frame could not be captured even under a paused clock with step_frames; /snapshot?step=1 (get_snapshot step true) queues the capture and one step together so the stepped frame's own paint services it.
created: 2026-10-06
completed: 2026-10-06
---

# Snapshot of a stepped frame as it was drawn

## Symptom

The one-frame flash of [before-render-phase] could not be captured over
the control API: `get_snapshot` after `step_frames` showed the settled
picture every time, so the glitch was found by reading the code and
confirmed by eye ([slate finch] 2).

## Cause

A snapshot queues a capture and latches a frame request; the capture is
serviced by the next paint. The request arrives as its own JS-thread
closure, after which flux runs the microtask checkpoint, so the paint
that services the capture always comes after the pending microtasks. A
paused clock paints through `render_now` with no JS frame, so even
"step, then snapshot" paints afresh after the step's frame has fully
settled. "Queue the capture, then step" had no spelling: the snapshot
latched its own frame first.

## Shape

- `request_snapshot` in `lattice/src/go/connection.rs` takes `step`: it
  refuses while the clock runs (a queued step would fire at the next
  pause instead), queues the capture, adds one clock step, then latches
  the frame request. All three happen in one closure on the JS thread,
  between two frame signals, so the stepped frame is the next one and
  its paint services the capture.
- The dev server's `/snapshot` takes `&step=1` (`packages/cli/src/server/
  control.ts`); `get_snapshot` takes `step: true`
  (`packages/cli/src/mcp/main.ts`); `agents/debugging.md` documents both
  next to the plain snapshot and in the stepping recipe.
- Verified on a dev server: the stepped capture replies with the frame
  (`pendingSteps` back at 0 afterwards), and with the clock running the
  query answers the refusal.

[slate finch]: ../feedback/slate-finch.md
[before-render-phase]: ../done/before-render-phase.md
