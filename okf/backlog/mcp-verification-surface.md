---
type: backlog-item
title: The MCP verification surface - input, clock, crop, props
description: Four gaps that recur across every external agent-built app report; together they are the difference between an agent verifying an app and guessing at it. Gaps 2-4 implemented 2026-08-06 (crop/scale, clock control incl. virtual timers, props/quad, plus server app identity); gap 1 (input injection) stays with its own item.
status: done
timestamp: 2026-08-06T00:00:00Z
---

# The MCP verification surface - input, clock, crop, props

Source: five external agent-built app reports (2026-08-04 to 2026-08-06,
issues extracted in okf/feedback/: [[cedar-lock]], [[velvet-acre]],
[[marble-fox]], [[tin-orchard]], [[willow-stamp]]). Core-only apps, all five
built end to end by an agent driving the app over `srt mcp`, on
0.0.44/0.0.45.

All five shipped. All five reported the same four gaps, independently. This
file bundles them because they are one story - an agent can *read* a running
app in detail and can *poke* app-internal state, but cannot drive input, cannot
stop time, cannot magnify, and cannot ask what a prop currently is - and
because the cheap ones partly cover for the expensive one, so the ordering
matters more than any single item.

Grouped by impact below; the proposed staging is by cost and deliberately
inverts it.

## The four gaps

### 1. No input injection (4 of 5 reports; one rates it "blocked")

Already owned by [[mcp-input-injection]] (status deferred, with round-2 text
and timed-sequence additions). Recorded here only as new evidence for its
priority: every one of these five apps shipped its entire interactive surface
unverified. Drag-to-rotate, tap-to-select, hover and wheel-zoom in the
wireframe viewer; paddle dragging across a 3D-transformed field; the whole
`onKeyDown -> held -> readInput()` chain in the shooter; `onKeyDown` bubbling,
`repeat` suppression, focus and `stopPropagation` in the engine port.

The point they all make about the `registerDebug` workaround is worth carrying
into that item: it does not test the interaction, it tests the code path
*behind* it, skipping hit-testing, event routing, `localX`/`parentX` resolution
and `pointerId` bookkeeping - which is the layer most likely to be wrong. One
report puts it sharply: it only works because you wrote the app yourself, so
for any app you did not write the interactive surface is undrivable.

### 2. No clock control (3 of 5 rank it #2 or #3)

MCP round trips are routinely longer than the thing being observed, so
animation verification is a race the agent loses:

- a 2.7 s death animation was uncapturable; three attempts all photographed the
  next life;
- points were scored *between* two consecutive tool calls (0-1, 0-2, 0-4 across
  three calls), so `serve -> snapshot` repeatedly landed on the serve screen;
- a ~500 ms camera tween settled before every `get_stats` call landed, so a
  real drag frame was never measured through the tool at all;
- a debug command that sets up state and returns immediately races the live
  frame loop, which had already reached the game-over screen by the next call.

The docs recommend per-app `seek`/`pause`/`play` debug commands; all four
reporters built them and agree the advice is good. But it is app-specific, only
works if you wrote the animation clock yourself, and an app animating on
runtime-paced `onFrame` cannot be frozen from outside at all. One report makes
the ownership argument directly: `onFrame` is runtime-paced, so the runtime is
the natural owner of pause/step, and it then works for every app without app
cooperation.

**This is smaller than it looks, because the mechanism exists.** In playback
mode the clock is already virtual and a pure function of the frame index
(`lattice/src/lib.rs:667` - `flux::Clock::new(|| playback_frame * 1000 / rfps)`),
and in run mode `PacedClock` (`lattice/src/paced_clock.rs`) already advances
exactly one refresh period per frame signal, with `FluxRuntime::frame()`
(`lattice/src/runtime.rs:273`) as the single place that publishes the frame
index, ticks the clock, flushes rAF and emits `render`. So `pause` is "stop
delivering frame signals" and `step_frames(n)` is "deliver n of them", both at
that one seam, with the clock advancing by construction rather than by a second
time source. Interaction to check: demand-driven rendering and the idle-tick
gate ([[idle-tick-gpu-backlog-runaway]]) also decide when frame signals happen,
so a pause must not be defeated by a frame request, and a step must produce a
present even when nothing asked for one.

Companion, same root cause on the observability side: `get_stats` returns an
instantaneous/smoothed snapshot and is therefore blind to exactly the
transients it exists to measure. Asked for as `get_stats { sample_ms }`
returning min/avg/max per counter, or persistent rolling peaks
(`peakFrameMs`, `peakSetPropsPerFrame`, `sinceReset`). Already covered as the
tracing/high-water-mark bullets in [[mcp-agent-loop-improvements]] theme 3; the
fifth report's `presentBlockedMs` ask belongs there too (a throttled desktop sat
at 15 fps / 62 ms frames with `jsMs` 1.7 and `paintMs` 0.4 while a Pi 4 ran the
identical bundle at 60 fps - diagnosing it required owning a second client).

### 3. `get_snapshot` cannot crop or scale (3 of 5)

`AGENTS.md` is candid that snapshots reach an agent downscaled and that
hand-authored geometry must be inspected magnified, and prescribes a per-app
`zoom` debug command ([[scaffold-zoom-debug-command]]). Two reports traced an
actual misdiagnosis to the missing tool parameter:

- three trail ghosts were read as a ground shadow, and the real bug was only
  found after building a frozen-frame debug path - "a 200x200 crop at 4x would
  have settled it in one call";
- an SVG arc `sweep=0` renders a teardrop rather than a pie, and at sprite scale
  in a downscaled capture the two are hard to tell apart.

The workaround is invasive: a wrapper `d-view` around the whole view tree plus a
signal plus a `registerDebug` call, shipped in app source, in every app. And
the asymmetry is the giveaway - `get_texture` already takes
`x`/`y`/`width`/`height`; the tool that renders app geometry is the one that
cannot zoom.

Sub-item already done: "capturing a `d-*` node cannot isolate a detached
sprite" was fixed by [[capture-detached-nodes]] (2026-07-29, captures size from
`local_bounds`); the report that raised it was on 0.0.44. Its remaining open
question - ink extent vs drawing surface - is what crop would paper over for
`d-path`.

Still open and worth one line in the tool description: a subtree capture
renders with no ancestor background. One report's first viewport capture came
back as a dark-blue wireframe on **white** and read as "the background is
broken". The current text says the window node "captures everything" but never
says a subtree capture excludes ancestor paint.

### 4. `get_render_tree` reports geometry but not props or transforms (2 of 5)

Called "the single biggest time sink of the build" by one report. The tree
returns `id`, `kind`, `x/y/width/height`, `text` and children, so there is no
way to ask a running app what a node's `rotate`, `color`, `d`, `scale` or
`opacity` currently is.

- To answer "is `rotate` being applied?" the reporter wrote a throwaway entry
  drawing the shape three times, `load`ed it, captured, compared, `load`ed the
  real app back, then repeated the exercise for the arc flags. Four reload
  cycles and a scrapped file to answer a question the runtime already knows -
  and `rotate` was fine, the *path* was wrong, which is exactly the
  misdiagnosis prop visibility prevents.
- With `perspective` + `rotateX` + `rotateY` live the tree reports enclosing
  axis-aligned boxes: a 1280-wide design space reported as `x -42.73, width
  1365.45`, the two paddles as 27.24 and 26.91. A transform is *detectable*
  (non-integer boxes, asymmetric widths) but not *readable*, so "did the tilt
  apply, and where does the near edge land" is answerable only from pixels. For
  a 3D-ish scene the tree stops being a verification tool exactly where it
  would be most useful.
- Smaller: a `d-text` reports the inherited box height (702 of 720) rather than
  its painted text bounds, so text placement is not verifiable from the tree
  either.

Asked for as current property values behind a `props: true` flag - the tool
already has `query`/`root`/`depth` for scoping payload - plus each node's
effective transform, or the four corners of its painted quad.

Adjacent and already filed: [[mcp-agent-loop-improvements]] section 5 adds a
`drawn` box for detached nodes from the same `local_bounds` quantity. Props and
the painted quad are the parts nothing covers.

## Proposed staging

Minimum first, which inverts the impact order: 3 is CLI-only, 2 reuses
machinery that exists, 4 is a serialization question, 1 is the real build.

1. **Crop + scale on `get_snapshot`** - `x`/`y`/`width`/`height` + optional
   output `scale`, matching `get_texture`'s parameters exactly. Verify the
   assumption first: the PNG arrives at the MCP layer full-resolution and is
   downscaled afterwards by the agent harness, so if that holds this is a crop
   and a nearest-neighbour upscale inside `packages/cli/src/commands/mcp.ts`
   with **no runtime, capture-path or protocol change at all**. Add the
   ancestor-paint sentence to the tool description in the same pass. Smallest
   possible change, removes the per-app `zoom` scaffolding, and closes two of
   the reported misdiagnoses.
2. **`step_frames(n)` and `set_time_scale(x)`** (0 = pause) at the
   `FluxRuntime::frame()` seam, per the mechanism note above. Makes every
   animated app verifiable without app cooperation, and makes the item-1
   workarounds observable in the meantime (a state-setup debug command is only
   worth calling if you can stop time before the next round trip).
3. **`props: true` on `get_render_tree`**, plus the effective transform or
   painted quad. Scoped by the existing `query`/`root`/`depth` so payload stays
   bounded. Sequence with [[mcp-agent-loop-improvements]] section 5 - same
   serializer, and the `drawn` box wants the same visit.
4. **Input injection** - [[mcp-input-injection]], unchanged in shape, with
   these five reports as the evidence that it stays the top ask. Landing 1-3
   first is not a substitute; it is what makes the eventual injection *usable*,
   since injecting a drag you cannot freeze or magnify verifies little.

Note that 3 and 4 pull in opposite directions on cost: after clock control, a
lot of "did this interaction work" questions become answerable with a debug
command plus a frozen frame, which is why input injection has survived being
deferred twice. It should not survive a third time - four independent sessions
now report shipping their entire interactive surface untested.

## Implementation (2026-08-06)

Stages 1-3 landed, plus a user-requested stage 0. What shipped, and the
decisions that differ from the proposal above:

- **Stage 0, app identity**: `/__control__/clients` (list_clients) now
  reports the server's `entry` and `projectDir`, so an agent can check it
  reached the right app before acting (fixed dev port; `load` moves the
  entry). This is the SERVER's truth only - a client switched via the
  on-device launcher is not caught; full per-client truth needs an
  engine-runner-owned running-app slot and belongs with the per-client
  load-outcome work (the load-reports-ok-while-a-client-failed report).
- **Crop + scale** went runtime-side, NOT CLI-side: the CLI has no PNG
  codec at all, while get_texture's crop already row-copies CPU-side after
  readback in `connection.rs`. Shared `crop_scale_rgba` (crop, then
  nearest-neighbour pixel duplication) now backs both get_snapshot and
  get_texture; `scale` is 1-8, output capped at 8192 px/side (the encode
  runs inline on the JS thread). Crop coordinates are captured-image
  (device) pixels. Tool descriptions gained the ancestor-paint warning.
- **Clock control** is two layers. `set_time_scale` (0 = pause) +
  `step_frames(n)` ride a new `clock` query into `ClockControl` atomics
  (runtime.rs), applied in `FluxRuntime::frame()`: a paused frame skips
  raf::flush and the render event but still runs the draw path natively
  (`draw::render_now`, the factored renderFrame body), so snapshots,
  captures, cameras and get_stats stay alive while app time stands still -
  the naive gate would have broken get_snapshot's paint-serviced capture.
  Steps advance exactly one period and latch a present. PacedClock gained
  an `offset` absorbing paused wall time (the GAIN correction would
  otherwise fast-forward after resume). Reload/stop reset the clock so no
  app boots under a stale pause.
- **Virtual time (beyond the proposal, user direction)**: we own flux, so
  the WHOLE JS time surface is one frame-stepped timeline now. flux gained
  `install_virtual_time`/`advance_virtual_time`: with it installed,
  setTimeout/setInterval live on a virtual deadline heap fired from
  advance (one advance = one task-queue turn; intervals collapse missed
  periods), and lattice installs it per engine and advances with the rAF
  timestamp each frame. performance.now() reports the paced clock (the
  run-mode flux::Clock is now paced-backed; the wall correction source
  moved into FluxRuntime). So pause freezes onFrame, rAF, timers and
  performance.now together; Date.now() is the wall escape hatch (docs
  updated: window.ts, flux-types time.d.ts, docs/flux.md). Headless flux
  (dev server, scripts) never installs it - tokio timers unchanged. This
  also closes playback's timer hole (timers were wall-time during
  playback) and is the seam automated tests drive later: construct flux
  with a manual advance and no GUI.
- **Props + quad**: reader `read_jsx` lives in
  `flux/src/plugins/gui/properties/read.rs`, NEXT TO apply_jsx (the naming
  authority - keep the two in sync; alloy stays JSX-free). Off-default
  values only. The painted quad came from splitting `compute_bounding_box`
  at its last line (`RenderTree::painted_quad`), emitted only when it
  differs from the AABB. `try_node` is now pub (read-only; mutation stays
  behind edit/try_edit).
- The section-5 `drawn` box of [[mcp-agent-loop-improvements]] did NOT
  ship: the tree's boxes already come from `local_bounds` via
  compute_bounding_box, so for kinds with explicit geometry the reported
  box IS the drawn box; what remains open there is ink extent for
  Line/Path (the capture-detached-nodes open question).
- Tests: alloy `painted_quad_*` (tree.rs), flux `tests/time.rs` (7 cases:
  deadline order, interval collapse, cancel-inside-callback, no rewind,
  one-turn-per-advance).
- Not touched: `srt mcp` and the dev server need a restart, and clients a
  rebuild, before any of this is visible end to end.

## Relation to existing items

- [[mcp-input-injection]] - owns gap 1 outright; this file adds evidence
  only. Still the top remaining ask, now three deferrals in.
- [[mcp-agent-loop-improvements]] - owns the `get_stats` sampling/high-water
  companion to gap 2, and the `drawn`-box neighbour of gap 4 (partially
  superseded; see the implementation note above).
- [[scaffold-zoom-debug-command]] - retired 2026-08-06 by the tool-side
  crop+scale.
- [[capture-detached-nodes]] - done; retired the sub-item of gap 3 about
  isolating a detached sprite.
- [[idle-tick-gpu-backlog-runaway]] and demand-driven rendering - the frame-
  signal gate that gap 2's pause/step has to cooperate with.

## External-verification follow-up (2026-08-08)

The first external verification pass ([[paper-crane]], 2026-08-07) confirmed
the whole surface end to end and produced five fixes, all landed:

- `props: true` now includes overflow/overflowX/Y (off-default; uniform name
  when the axes agree). The v1 "no layout props" scope line cost the reporter
  the clip-bug diagnosis - an absent prop read as "never landed". Reader gets
  the style via the new `Element::style()` accessor; round-trip pinned in
  flux/src/tests/properties.rs on the report's exact overflow+viewBox combo.
- The unknown-query-kind reply no longer leaks Rust debug formatting
  (`Some("input")`); it names the kind and the client's own runtime version
  so a mixed-version fleet self-diagnoses.
- The client `info` message advertises `queries` (supported query kinds,
  QUERY_KINDS const beside the match); /clients and list_clients surface it.
  Closes the query-planning ask ([[willow-stamp]] item 5, re-raised by
  [[paper-crane]]); an empty list = a runtime
  predating the advertisement.
- `load` with an entry outside projectDir now names the real constraint
  instead of the bundler's "bun install" advice.
- The multiple-clients error speaks tool vocabulary (client id +
  list_clients), and CLIENT_ARG documents that the default only exists with
  a single client attached.

Still open against this surface: an audio counterpart to
get_gpu_resources, and a per-subtree kind histogram on get_render_tree.
send_input's text kind remains unverified end to end (blocked outside this
surface). The report's remaining findings concern other surfaces and are
not tracked here.
