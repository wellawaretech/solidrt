---
type: backlog-item
title: MCP improvements and expansion (agent dev-loop, round-2 feedback)
status: deferred
timestamp: 2026-07-18T00:00:00Z
---

# MCP improvements and expansion

Source: app-port feedback round 2 (2026-07-17 session), findings 8, 9, 10 and
the tooling companions of 11. The MCP inspection loop itself was rated a
highlight (srt check -> reload -> get_logs -> get_snapshot, registerDebug as
the standout); these items are the friction that remains. Grouped by theme,
roughly cheap-to-expensive within each.

Status 2026-07-18: theme 1 shipped (readOnlyHint annotations on the nine
inspection tools + AGENTS.md permissions paragraph), along with theme 2's
AGENTS.md multi-client paragraph. All tools also carry openWorldHint: false
(local dev server only), and watch adds destructiveHint: false +
idempotentHint: true - mutating but benign, NOT readOnlyHint, so a
hint-honoring harness never auto-approves a behavior change. Theme 2's code bullets (call_debug
`client: "all"`, list_clients form-factor fields) and themes 3/4 remain.
Same session also added an MCP `load` tool (POST /__control__/load): set the
entry + file-serving root and rebuild-push, so an agent can start or switch
the app instead of only reloading one. Caveat: the srt process is not told,
so a watcher started on the launch-time source keeps watching that file.

Also added (not from the report): MCP `watch {enabled}` tool. srt's file
watcher previously auto-reloaded on agent-created files (agent in-place
edit events happened to be dropped by Bun's recursive fs.watch on Linux -
an accident of atomic-save event coalescing, not design). Now the agent
pauses auto-reload before touching files (flag latched on the server via
POST /__control__/watch, watcher reads GET /__internal__/watch per event);
a successful /reload or /load re-enables it, repl `watch on|off` is the
manual override, and a suppressed change prints to the srt console.

## 1. Permission prompts (finding 8)

Agent harnesses ask approval per MCP tool, so a session eats a dozen prompts
before the loop flows. Permissions are harness policy; no per-agent config
files shipped. Two protocol/docs changes cover it:

- Add MCP-standard `annotations` to the TOOLS table in
  `packages/cli/src/commands/mcp.ts` (today only description + inputSchema):
  `readOnlyHint: true` on the inspection tools (list_clients, get_logs,
  get_render_tree, get_snapshot, get_stats, get_gpu_resources, get_texture,
  get_buffer, list_debug); leave reload and call_debug unannotated. Clients
  that honor the hint auto-approve the read-only majority.
- Append a permissions paragraph to the MCP section of scaffold/AGENTS.md:
  all tools only talk to the local dev server, nothing leaves the machine;
  agents should not work around prompts but tell the user they can
  pre-approve the server once in their own agent's settings. The report has
  ready-to-paste wording.

## 2. Multi-client ergonomics (finding 9)

With desktop + phone attached, the whole session ran against one client; the
phone sat on the initial screen, indistinguishable from a crash to the human
holding it. Three fixes:

- call_debug `client: "all"`: broadcast, results keyed by client id. Every
  connected screen then moves in lockstep with the agent; removes the
  incentive to single-thread.
- list_clients: add windowSize, displayScale, orientation. Today nothing
  signals that another client is a different form factor worth its own
  verification pass.
- scaffold/AGENTS.md paragraph: reload pushes to all clients but
  call_debug/get_snapshot/log cursors are per client and interactive state
  does not sync; drive state on every client (or say which one you are
  using), and snapshot each distinct form factor before calling a visual
  change done. Ready-to-paste wording in the report.

## 3. Interaction performance visibility (finding 10)

A human immediately felt typing jank (controlled TextInput refiltering a
fully-mounted 184-row list per keystroke); the agent never noticed and could
not reproduce or measure it when told. Verified gaps: call_debug sets signals
directly, bypassing focus/key/TextInput - the exact path where input latency
lives - and ~1s per MCP round-trip cannot approximate a 10-keys/sec burst;
get_stats frame times are smoothed (one-frame 84ms hitch averages away) and
the layout counters cover only the last rebuild, overwritten before the next
call arrives (measured: a ~170-row remount left layoutMs 0.01, paraShapes 4
one round-trip later). Nothing flags slow frames in the console.

In decreasing value:

- inject_input: covered by the existing [[mcp-input-injection]] item; round 2
  adds text events and timed sequences (e.g.
  `[{text:"g"},{text:"o",delayMs:80},...]`), noted there.
- Interval tracing: start_trace/stop_trace (or `record_stats durationMs`)
  returning per-frame data or a summary - p50/p95/max frame ms, dropped-frame
  count, worst frame's phase breakdown + rebuild counters. Flow: start ->
  inject typing -> stop -> read "9 frames over 32ms, worst 84ms: layout 71ms,
  paraShapes 3900". The output names the fix.
- Slow-frame console warnings: throttled runtime warning when a frame blows
  its budget, phase breakdown inline. Zero new tools; jank becomes visible
  through get_logs.
- Cheaper middle ground: high-water-mark fields in get_stats (worst frame +
  its counters over the last N seconds). One stats read during the session
  happened to land mid-remount and showed frameMs 427 / fps 0 / cpuPct 93,
  so the latched stats CAN show a storm - it is just luck today.

## 4. Leak/lifecycle diagnostics (finding 11 companions)

Owned by [[unmount-node-leak]] (listed there as companion tooling), noted
here because they grow the same stats/debug surface:

- get_stats node breakdown: mounted vs total, making the orphan population
  explicit (get_render_tree walks ~100k mounted while nodes counted 125k;
  the ~25k gap was invisible).
- Debug command to force GC / flush disposals, separating "not yet
  collected" from "actually leaked".
- Dev-build leak sentinel: warn when live nodes grow monotonically across N
  full rebuilds at a stable tree shape.

## Relation to existing items

- [[mcp-input-injection]] - the inject_input half of theme 3.
- [[production-diagnostics-surface]] - production consumer for the same
  counters; the tracing/high-water items here are the dev-loop consumer.
- [[unmount-node-leak]] - owns the theme-4 items.
