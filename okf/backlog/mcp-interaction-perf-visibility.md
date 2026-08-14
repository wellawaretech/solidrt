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

Split out of a five-part round-2 agent dev-loop feedback item when okf was
restructured; the siblings are
[mcp-multi-client-ergonomics](mcp-multi-client-ergonomics.md) and
[mcp-detached-node-bounds](mcp-detached-node-bounds.md).
