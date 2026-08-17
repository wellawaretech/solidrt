---
title: Jank an agent cannot see or measure
description: A human immediately felt typing jank that the agent could not reproduce or measure: get_stats frame times are smoothed so a one-frame 84ms hitch averages away, layout counters cover only the last rebuild and are overwritten before the next call, and nothing flags a slow frame anywhere.
created: 2026-07-27
---

# Jank an agent cannot see or measure

What it looks like when you hit it: someone types in the app and feels it
stutter, tells the agent, and the agent finds nothing wrong. Measured on a
controlled TextInput refiltering a fully-mounted 184-row list per keystroke.

The gaps are structural, not a missing number:

- `get_stats` frame times are smoothed, so a single 84ms hitch averages away.
- The layout counters cover only the last rebuild and are overwritten before
  the next call arrives - a ~170-row remount measured `layoutMs 0.01`,
  `paraShapes 4` one round trip later.
- Nothing flags a slow frame in the console, so `get_logs` shows nothing either.
- An MCP round trip is roughly a second, which cannot approximate a
  10-keys/sec burst even now that synthetic input exists.

In decreasing value:

- **Interval tracing.** `start_trace`/`stop_trace` (or `record_stats
  durationMs`) returning per-frame data or a summary: p50/p95/max frame ms,
  dropped-frame count, and the worst frame's phase breakdown plus rebuild
  counters. The flow is start -> inject typing -> stop -> read "9 frames over
  32ms, worst 84ms: layout 71ms, paraShapes 3900". That output names the fix.
- **Slow-frame console warnings.** A throttled runtime warning when a frame
  blows its budget, with the phase breakdown inline. Zero new tools - jank
  becomes visible through `get_logs`.
- **High-water-mark fields in `get_stats`** as a cheaper middle ground: the
  worst frame and its counters over the last N seconds. One stats read during
  the session happened to land mid-remount and showed frameMs 427 / fps 0 /
  cpuPct 93, so the latched stats CAN show a storm - it is luck today.

The synthetic-input half of this has since landed (`send_input`, including text
events and timed sequences), so what remains is purely the measurement side.

## The GPU-health fields point the wrong way (2026-08-17)

Same tool, different failure: `get_stats` was used to attribute a latency
report to either the app or the machine, and three things about the payload
sent the reader the wrong way. Read against the field docs in
`packages/cli/src/commands/mcp.ts`.

- **`rasterQueue` read inverted.** Documented as "stuck nonzero means the
  raster thread is backlogged". Measured on a release win32 client, same
  app, same session, two ~11 s windows: idle (nothing moving) `rasterQueue`
  89, `gpuPasses`/frame ~4.7, `gpuPassMs`/frame ~1.8, fps 60; under real
  work (physics + particles) `rasterQueue` 43, passes/frame ~2.3, fps 60-61.
  It sat at exactly 89 across three consecutive idle samples while
  `gpuPasses` climbed and fps held, so frames were plainly completing - and
  then HALVED when the app started doing work. Following the documented
  reading the conclusion was "GPU-saturated" while target renders took ~5%
  of wall clock. This tree's accounting (`RasterSender::send` increments
  before send, the raster loop decrements after every arm, no early
  `continue` on the outer loop) looks balanced, so either something on the
  ANGLE/win32 path leaks the counter or the field genuinely counts
  something else while idle; verify on Windows before touching the docs.
  As it stands the field is worse than absent: it reads highest exactly
  when nothing is happening.
- **`fenceTimeouts` is cumulative-only.** It is the field that actually
  answers "is the GPU over budget RIGHT NOW", but with no rate a single
  sample cannot tell "over budget now" from "was, ten minutes ago". A stale
  51 next to a suggestive `rasterQueue` is most of why the situation was
  misread; every neighbouring field (`fps`, `frameMs`, `jsMs`,
  `rasterQueue`) is instantaneous, so the mix invites exactly this mistake.
- **No clock in the payload.** Deriving a rate from the cumulative fields
  needs an external timer: no timestamp, no frame counter. `idleTicks`
  looks like it might serve and does not (advanced 26 over ~11 s at 60 fps).

Wanted, and it composes with the tracing above:

1. Fix `rasterQueue`, or document what it counts while idle.
2. Ship rates beside the cumulatives (`fenceTimeoutsPerSec`,
   `gpuPassesPerFrame`, `gpuPassMsPerFrame`): every caller derives them the
   same way, and deriving them needs state the caller has to carry between
   calls.
3. A timestamp and a frame counter in the payload, so two samples can be
   differenced without trusting the caller's wall clock.
4. A single derived verdict - "GPU over budget" / "raster backlogged" /
   "healthy" - because the individual counters demonstrably do not compose
   into one for a reader who is not the engine author. The paced-timeline
   drift from [frame-driver-pacing-contract](frame-driver-pacing-contract.md)
   stage 3 belongs in the same payload.

Split out of a five-part round-2 agent dev-loop feedback item when okf was
restructured; the siblings are
[mcp-multi-client-ergonomics](mcp-multi-client-ergonomics.md) and
[mcp-detached-node-bounds](mcp-detached-node-bounds.md).
