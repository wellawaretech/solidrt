---
title: A streaming tile world hitches on the tablet whenever a RESIDENT chunk re-bakes, a write into a texture the frames in flight still sample
description: The tiles example on the SM-T500 shows 50 to 67 ms gaps a few times a second; measured 2026-10-16 with a probe matrix and a sched/gfx trace, the trigger is the beacon blink's re-bake of resident chunks, a batch of GPU passes the Adreno 610 pays beside a near-budget frame (0.7 to 1.7 ms each, plus a ~27 ms drain when the target is one the in-flight frames sample); fixed by baking into retired targets (the pool ages its gives by the frames in flight) and pacing bakes to a per-frame budget shared by every tile layer (default one), which reads clean on the tablet; the TV reading and the startup burst remain.
created: 2026-10-16
---

# A streaming tile world hitches on the tablet whenever a resident chunk re-bakes

## Symptom

`packages/2d/examples/tiles.tsx` on the tablet (SM-T500, Adreno 610,
2000x1200 at display scale 1.5, Player.dev of 2026-10-15): the ship flies
at 60 fps, then every one to three seconds the picture holds for three
or four refreshes. Read through the control API
(`/stats?client=<android>&windowMs=5000`, `/logs` for the example's own
`[hitch]` lines, which log any onFrame gap past 1.5 refresh periods),
with the cadence hold OFF (`--es sol_env SOLIDRT_CADENCE_HOLD=off`, see
the second finding below for why):

| state | fps | missed presents / 5 s | GPU ms per frame | hitch lines / 5 s |
|---|---|---|---|---|
| example as is | 58 to 60 | 3 to 11 | 15.4 | 1 to 4 of 50 to 67 ms |
| Pixel 7 (Mali-G710, 90 Hz) | 90 | 0 | 1.7 | 0 |
| laptop (Intel RPL-P, Mesa) | 60 | 0 | 2.9 | 0 |

The GPU frame sits at 15.4 ms of the 16.7 budget because the main view
fills the window, the display scale is 1.5, the 2d oversample is an
integer, so the auto pick rounds up to 2 and the view target is 2666 x
1600 (4.3 MP). That is headroom, not the cause: the same 14.7 ms frame
with nothing baking misses nothing (row "frozen, no blink" below).

## Findings (2026-10-16, probe matrix on the tablet)

`probes/2d-tile-hitch-probe.tsx` is the example with knobs (pool
hand-over aging, mip chain, camera frozen, beacon blink, oversample
cap), each variant pushed by reload-on-save and read over three 5 s
windows after a settle. Hold off throughout.

| variant | enters / 5 s | missed / 5 s | GPU ms | hitch lines / 5 s |
|---|---|---|---|---|
| flight, blink (the example) | 26 to 33 | 3 to 11 | 15.4 | 1 to 4 |
| flight, blink, pool aged 8 gives | 29 to 35 | 8 to 15 | 15.5 | 3 to 6 |
| frozen camera, blink | 0 | 8 to 9 | 15.1 | 2 to 4 |
| frozen camera, blink, mipmap off | 0 | 12 to 14 | 15.8 | 1 to 3 |
| frozen camera, no blink | 0 | 0 | 14.7 | 0 |
| flight, no blink | 31 to 38 | 0 | 14.4 | 0 |
| flight, no blink, pool aged 8 gives | 30 to 38 | 0 | 14.3 | 0 |

So: chunk ENTERS never hitch (six to seven a second, the just-left
texture handed straight over by the pool, 60 fps and zero misses). The
mip regeneration is not it. The pool's warm hand-over is not it. The one
trigger is the beacon blink: `setTile` on cells of RESIDENT chunks, whose
flush re-bakes those chunks with `renderTarget` into the texture they
already have, the texture the one or two frames still in flight sample
(`PRESENT_FENCE_DEPTH` is 2). An entered chunk's first bake writes a
texture no in-flight frame reads (the pool had held it for a few hundred
ms, or it is new), which is why enters are free.

The trace (`atrace gfx view sched`, 6 s of the frozen-with-blink state)
shows where the time goes: the frame that carries a re-bake completes on
the GPU in 42.6 ms where its neighbours take 12 to 14 (the `GPU
completion` fence wait), and the raster thread then sits 34.8 ms in
`queueBuffer` behind it. Nothing on the CPU side is long (sol-ui's and
sol-raster's own work stay under 3 ms). So the Adreno 610 resolves the
write-after-read hazard by draining the pipeline: roughly two frames'
worth of queued work (~27 ms) before the bake may write, and that frame
misses two or three refreshes. Mesa and the Mali-G710 resolve the same
hazard cheaply.

The rule this gives the engine: **never write a GPU resource a frame in
flight reads**. A bake, an upload (`uploadTexture`, `writeBuffer`,
`copyTexture`) or a resize into a resident resource is the hazard; a
write into a resource released at least the in-flight depth ago is not.
3d-world-residency's model uploads meet the same rule.

### Second finding: the cadence hold locks in after the startup burst

In the default Auto policy the example's startup (38 target creates and
40 bakes in a few frames) raises the hold, and it then stays up for
minutes of flight the device could run at full rate: the tablet at hold
3 (20 fps) on 2026-10-16, the Pixel 7 at hold 3 (30 fps at 90 Hz) with
a 1.3 ms GPU frame on the fixed build the same day, while a relaunch
that never tripped the rise ran at 90 fps; the tablet's Auto run on the
fixed build locked in 150 s after launch, past a two-minute reading. The controller's CPU term
under the hold holds the buffer-queue wait the held display causes, so
the step-down prediction never fits: okf/backlog/cadence-hold-dvfs-lock-in.md
(the diagnostics, the mechanism, two leads; the DVFS reading there was
the first guess).

### Third: the startup burst

The whole resident set (40 chunks) is wanted on the first frame; the
gate's default budget of 16 enters per frame means 16 blocking target
creates plus bakes per frame for three frames, 51.7 ms of work on the
first. Bounding it is pacing creates, not bakes: a smaller first-frames
budget, or pre-warming the pool. Not the hitch, but in "done looks
like".

## Fix (built 2026-10-16)

Three pieces, two rules: never write a GPU resource a frame in flight
reads, and never put more bake passes in a frame than the GPU pays
beside it.

The runtime says when a frame has retired. The raster thread already
keeps a fence per present (`PRESENT_FENCE_DEPTH`); each one now carries
the UI frame number its `RasterCmd::Frame` was built under (the present
index the frame loop stamps the signal with, `frameNumber()` in JS), and
when a fence signals the thread publishes `retired_frames = frame + 1`
(`RasterStats`, `/stats` `retiredFrames`, `flux:gpu` `retiredFrames()`):
every frame numbered below it has completed, since the GL stream
completes in order. A fence that times out publishes nothing, so under
the TV's saturated GPU the count simply lags. A headless or capture frame
has no present; it waits for its own work after the draw (bounded, like
the pacing wait) and publishes the same way, so tests see retirement.

The pool applies it. `createResourcePool` (core gpu.ts) gives pool.ts a
`retire: { submitted, retired }` pair of counts (`submittedFrames()`,
frames handed to the raster so far, `Context::submitted_frames` raised
at every submit; `retiredFrames()`); a give is stamped with the
submitted count, and `take` hands out only an id whose stamp the retired
count has reached, warmest first, else misses and the consumer creates.
The stamp is the submitted count, not the frame number, so a release
after an idle stretch (frames the demand gate never drew) is free at
once, and a release inside a frame waits for the frames already on the
GPU, never for its own. pool.ts stays pure (the headless test passes its
own counts). `app.settle()` and the control API's settle now also wait
for the raster thread to run everything sent (a submitted frame is work
in flight), so a test at rest sees `retiredFrames == submittedFrames`
and the hand-over test holds deterministically.

The tile layer bakes into retired sets. A chunk's GPU side (`fresh`
while never drawn) bakes in place; a re-bake of a drawn chunk (a
`setTile` into it, a tint, a params or density change) gives its set
back and takes a retired one (`takeResources`), re-uploads, bakes, and
the views rebind (`syncView`'s `setDrawTextures`). An entered chunk's
first bake therefore never meets a reader either: the gate's leave
before the enter still runs, the evicted set just waits a frame.

The tile layer paces bakes, the way the gate paces enters. One flush
serves every tile layer in the app (`flushLayers` in tiles.ts: a layer
registers its part, the microtask between frames or the frame's publish
pass runs it), and it spends ONE budget of bakes per frame across the
layers, `setBakeBudget(n)` in @solidrt/2d (the residency budget one
level up; two layers edited together spend it, never each their own). A
chunk's first bake, its appearance, is never held back - the gate paced
it, a hole is worse than a late refresh - but it spends the budget; the
re-bakes of drawn chunks take what is left, nearest a view's eye first
across the layers (the gate's urgency: eye distance in `leave`s, to the
nearest image under wrap), and the rest wait a frame showing their last
bake, the layer demanding the frames they need (`requestFrame`; the
loop is demand-driven). The spend is per frame of GPU work, in two
slots: what a frame's publish pass issues joins that frame, what a
flush between frames issues joins the next, and the next frame's start
(the `frameStart` event) carries the second slot over. Every frame that
runs resets the spend whether or not it drew - a window keyed on
submitted frames livelocked in a test, where a frame whose publish pass
found no budget left drew nothing and so never opened a new window.
`/stats` reports `app.tileBakes` and `app.tileBakesWaiting`. The default
is ONE bake per frame, the measurement below; `Infinity` is the unpaced
behavior. Verified by a test that steps frames: one re-bake a frame
nearest the eye first while the rest keep their pixels, six enters in
one frame all baked (red cells, not a pooled target's white) with the
re-bakes waiting, the microtask after that frame issuing the next
frame's one bake early and that frame's publish pass none.

Not a fix: a fractional oversample (headroom, not the cause), deferring
a bake a frame (the reader is still in flight), the retired sets alone
(the residual below), a budget of four (the first draft, from the
frozen-camera table below; it misses presents on every bake frame, as
unpaced does).

## Verified and what remains (2026-10-16, tablet, hold off)

After the first two pieces (retired sets, no pacing): stock example,
after the startup burst, five-second windows: 58 to 60 fps, 3 to 9
missed presents, one hitch line per window at first reading; a quiet
minute (no control-API traffic) logs 4 gaps of 66 to 83 ms against 20 to
40 before. The pool does its job (`/stats`: pool hits climbing, misses
flat, `submittedFrames - retiredFrames` = 2 to 3). The first mechanism
is gone: frozen camera with the blink went from 2 to 4 hitch lines and 8
to 9 misses per window to about one and 3 to 9, and extra pool aging on
top (eight gives) changes nothing more.

What remained scaled with the re-bake batch. Frozen camera, blink every
500 ms, re-baking N resident chunks per blink, no pacing:

| chunks per blink | missed / 5 s | hitch lines / 5 s |
|---|---|---|
| 1 | 0 | 0 |
| 4 | 0 | 0 |
| 6 | 4 to 8 | 0 to 1 (50 ms) |
| 8 | 4 to 8 | 1 to 2 (50 to 67 ms) |
| about 10 (all 48 beacons) | 6 to 9 | 1 (67 to 83 ms) |

So this GPU pays a few milliseconds per 480 x 480 bake pass, and a
batch past a handful in one frame overruns it. The `/stats` window's
per-target pass timers put it at 0.7 to 1.7 ms per bake (`targets[]`
exec over passes for the chunk targets), varying with the GPU clock.

With the pacing (the third piece), `probes/2d-tile-hitch-probe.tsx`
with `BAKE_BUDGET` and `FREEZE` as knobs, hold off, three 5 s windows
each, repeated A/B/A/B for the flight rows:

| state | budget | fps | missed / 5 s | GPU ms per frame | hitch lines / 15 s |
|---|---|---|---|---|---|
| flight, blink | unpaced | 57 to 61 | 3 to 12 | 14.2 to 15.5 | 7 to 18 |
| flight, blink | 4 | 58 to 59 | 8 to 12 | 15.7 | 17 |
| flight, blink | 2 | 59 | 8 to 10 | 15.8 | (log wrapped) |
| flight, blink | 1 | 60 to 62 | 0 | 14.2 (16.7 once, pegged) | 0 |
| frozen, blink | unpaced | 58 to 60 | 6 to 11 | 14.7 to 15.4 | 18 |
| frozen, blink | 4 | 57 to 59 | 14 | 15.9 to 16.0 | 18 |
| frozen, blink | 1 | 60 | 0 | 14.2 to 14.3 | 0 |

Two and four a frame are no better than unpaced: more frames carry
bakes and each of them overruns the period. One a frame is clean, and
the GPU frame then reads 14.2 ms (or settles at exactly the period, the
clock governor fitting the steady load, still with no miss). The
stock example state at the default (flight, blink, budget 1), a full
minute of twelve windows: 60 fps, 0 missed presents in every window, 0
hitch lines, `tileBakesWaiting` 0 between blinks (up to 8 inside one).
The first "done looks like" line holds.

Pixel 7 at the default: GPU 1.5 to 1.8 ms per frame, 0 to 4 missed
presents, no pacing pressure; it sits at hold 2 (45 fps at 90 Hz), the
cadence lock-in of okf/backlog/cadence-hold-dvfs-lock-in.md, not this
item. Tablet relaunched with the hold on Auto: hold 1 through the first
50 s (6 missed presents in the first window, the startup burst, then 0
to 2), 60 fps; the earlier lock-in came 150 s after launch, so a longer
read belongs to the cadence note.

What remains:

- The TV reading (armeabi-v7a, 50 Hz cap) before closing.
- The startup burst: the first window after launch still misses 6
  presents (16 blocking target creates per frame for three frames; the
  bakes themselves are now one a frame). The lever is pacing creates or
  pre-warming the pool, the third finding above.
- A GPU where every present fence times out (the TV's saturated case)
  never advances `retiredFrames`, so the pool hands nothing out, every
  take misses and creates, and the cap destroys the oldest held: a
  degenerate churn mode the fence-based count has by design. Worth a
  fallback (a depth-based age when fences stop signalling) once the TV
  reading shows whether it bites.
- The default of one bake a frame is the low-end baseline's; a desktop
  app that wants a whole-layer tint in one frame raises
  `setBakeBudget`, with a deeper `pool` past sixteen chunks. A
  per-device default (or a cost budget off the pass timers, which do
  read per-bake cost on this device) is a later refinement, not needed
  for the baseline.

Also seen: the probe reads `app.poolHits` as 0 through `/stats` while
its `poolStats()` counts normally, where the example reads both the
same; a bundling split between the probe (outside packages/) and core,
worth a look before trusting `app.*` from a probe. (Not reproduced on
2026-10-16 with the pacing build: `app.poolHits` climbed normally.)

## Done looks like

The tiles example on the tablet, hold off: no `[hitch]` line in a minute
of flight with the beacons blinking, missed presents in a five-second
window at zero (the "flight, no blink" row). Then the same at the Auto
hold, which must settle to 1 within the first seconds (the second
finding). The startup burst bounded (no cadence hold past 2). The same
reading on the TV (armeabi-v7a, 50 Hz cap) before closing.

## Involves

`packages/2d/src/tiles.ts` (the shared flush, `setBakeBudget`, the
re-bake swap, `applyDensity`), `packages/2d/tests/tiles-residency.test.tsx`
(the pacing test), `packages/core/src/pool.ts` (aging gives by frames in
flight) and its test `tests/pool.test.ts`, `alloy/src/raster/frame.rs`
and `mod.rs` (the fence-published retirement), `packages/2d/AGENTS.md`
(both rules), `probes/2d-tile-hitch-probe.tsx` (the knobs), and
`okf/backlog/3d-world-residency.md` (the same rules for model uploads).
The cadence lock-in: `alloy/src/cadence.rs`, `okf/design/frame-timing.md`.
