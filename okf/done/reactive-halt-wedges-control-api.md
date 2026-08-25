---
title: A reactive halt wedges the control API queries
description: After an uncaught error at mount, /snapshot timed out while /tree and /stats kept answering, because the render subscription lives in onSettled and a halted mount never settles; closed by render()'s root error boundary, which settles the mount and shows the error window instead of halting.
created: 2026-08-19
completed: 2026-08-23
---

# A reactive halt wedges the control API queries

## Symptom as reported

An app error halts the reactive system (REACTIVITY_HALTED). The client stays
alive and the error is in /logs, but `/tree` and `/snapshot` (and the MCP
tools over them) time out: "the client is connected but did not answer". An
agent or developer then debugs a phantom hang instead of reading the actual
error, and only a client restart recovers the tooling.

## What the repro showed (2026-08-23, solid-js 2.0.0-rc)

probes/reactive-halt-probe.tsx drives the halt from four places and reads
the outcome over the control API:

| halt trigger                          | halted | /tree | /stats | /snapshot | debug call |
|---------------------------------------|--------|-------|--------|-----------|------------|
| signal flip from a debug exec closure | yes    | 4 ms  | ok     | ok        | ok         |
| signal flip inside onFrame            | yes    | 4 ms  | ok     | ok        | ok         |
| signal flip from a pointer handler    | yes    | 4 ms  | ok     | ok        | ok         |
| throw during the initial mount        | yes    | 1 ms  | ok     | 504 (5 s) | ok         |

So only `/snapshot` wedged, and only after a mount-time halt. The `/tree`
timeout in the report was observed 2026-08-19, one day before 9bc5d4ef
rewrote the engine loop's microtask checkpoint; it did not reproduce on the
current loop and was not chased further.

## Mechanism

Not the one the report guessed (queries answered "in step with the frame
loop"). `/tree`, `/stats` and the debug calls are exec closures the engine
loop services independently of frames, which is why they answered. The
`"render"` subscription and the bootstrap first frame are installed inside
`onSettled` (packages/core/src/window.ts). A halt during the initial flush
means settle never happens, so the new engine never subscribes to render,
runFrame never runs, the tree never lays out (0x0), and a snapshot capture,
which is serviced on a paint, waits forever while the previous app's last
frame keeps presenting.

## Fix

render() wraps the app in an error boundary (okf/plans/
reactivity-halt-containment.md). A mount-time error resolves the boundary
to the error window instead of halting, the flush settles, the render
subscription is installed, and `/snapshot` of the (error) window answers:
verified 200 on the same probe. The "halted" marker the report wanted is
moot: there is no halt anymore, the error is a window in the tree and one
`Uncaught error` log line.
