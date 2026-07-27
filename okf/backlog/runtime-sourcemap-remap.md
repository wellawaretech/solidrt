---
type: feature-proposal
title: Runtime-side sourcemap remapping
description: Remap stack frames in the runtime itself so the local terminal and logcat show tsx positions too; explicitly not to be done unless server-only remapping proves insufficient.
status: deferred
tags: [flux, lattice, sourcemaps, dev-tooling, logging]
timestamp: 2026-07-15T00:00:00Z
---

# Runtime-side sourcemap remapping

This item exists so the design is not lost, NOT as a green light. The
explicit decision (2026-07-15) was: keep sourcemap remapping server-only and
dogfood it. Pick this up only if raw `main:LINE:COL` frames in the local
terminal or logcat turn out to be a real, recurring pain in practice - not
because the item is here.

# Current state (what already works)

Dev bundles carry a composed sourcemap (bundle -> original .tsx). srt sends
it alongside every reload; the dev server latches it (`state.currentMap`)
and rewrites `main:LINE:COL` frames in forwarded client logs before
buffering (`packages/cli/server/remap.ts`, wired into `appendLog` in
`control.ts`). So MCP `get_logs` and everything reading the server's log
buffer already sees `src/app.tsx:42:7` positions.

# The gap this would close

The runtime itself (QuickJS) knows nothing about sourcemaps, so surfaces
that print without going through the server show raw bundle positions:

- the local client's own stdout (the terminal a developer watches during
  `srt run`)
- Android logcat

# Sketch

- Ship the map to the client in the reload message (dev only). Strip
  `sourcesContent` first - it roughly doubles the payload and remapping
  only needs the mappings.
- Remap in the engine logger path (Rust) so frames are rewritten once,
  before both printing and forwarding. Server-side remapping then becomes
  redundant but harmless (its regex simply finds no `main:` frames).
- Use sentry's `sourcemap` crate (safe binding, no self-authored parsing).
- QuickJS frames are `at name (main:LINE:COL)`, 1-based line AND column;
  sourcemap columns are 0-based (col-1 on lookup, col+1 on print). The
  server-side `remap.ts` is the reference implementation.

# Why not the cheaper srt-side pipe trick

Considered and rejected: srt holds the map and spawns the local client, so
it could line-buffer the child's stdout and rewrite frames in the pipe. But
that requires switching the child from `stdio: inherit` to a pipe (the
client loses its TTY, changing color/formatting behavior), needs line
buffering against chunk-split frames, and still leaves logcat raw - real
regressions for half the problem. If the gap ever justifies work, do the
runtime-side version.

# Costs to weigh when picking this up

A dev-only sourcemap dependency and code path inside the runtime (cuts
against the "runtime is foundation, tooling adapts" layering), map bytes in
every dev reload to every client (including tunneled devices), and runtime
binary rebuilds across all platforms.
