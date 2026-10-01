---
title: A headless render is not reproducible when the app draws random numbers
description: srt render stepped time deterministically but left Math.random on the engine's clock-seeded generator, so an app with particles, a shake or a shuffled list rendered other frames on every run; closed by the render host seeding the context with flux's seed_random (fixed by default, srt render --seed <n>) and freezing the wall like a test.
created: 2026-09-30
completed: 2026-10-01
---

# A headless render is not reproducible when the app draws random numbers

## Symptom

Two runs of `srt render` on the same app write different frames when the
app calls `Math.random()`: a particle burst, a camera shake with no
direction given, a shuffled list. Time is not the cause. Playback steps
the frame clock and the virtual timers, so everything keyed on time is
the same on every run; the random numbers are not, because the engine
seeds its generator from the clock in microseconds when the context is
created.

It matters where frames are compared: an image golden, the capture-based
tests of the test harness's stage 5
([test-harness](../plans/test-harness.md)), a recorded input played back
to reproduce a bug.

## What exists

`flux::seed_random(ctx, seed)` (`flux/src/standards_plugins/random.rs`)
makes a context's `Math.random` a reproducible sequence, and seeds the
isolates that context spawns. `flux:test` is its only host today; it
seeds every test.

## Done looks like

- Playback mode seeds the app's context, with a fixed seed when nothing
  is said: the mode exists to be reproducible, so reproducible is what an
  app that says nothing gets.
- `srt render --seed <n>` renders on another sequence.
- Two runs of a render of an app that draws random numbers write the
  same frames; a test says so.
- The dev client in run mode is untouched: an app a person looks at keeps
  the engine's generator.

## Involves

- lattice: one `builder.plugin(move |ctx| flux::seed_random(&ctx, seed))`
  beside the `install_virtual_time` plugin (`lattice/src/lib.rs`), in
  playback mode only, the seed carried by `PlaybackConfig`.
- the CLI: the `--seed` option on `render` (it exists on `test`), passed
  to the runner.
- Decide whether a reload in a dev session under a frozen clock
  (`/clock?scale=0`) should be seedable over the control API as well;
  the app layer of the test harness seeds through `srt:test` and does
  not need it.

## Closed (2026-10-01)

Closed by step 4.6 of [test-harness](../plans/test-harness.md) (D35):
playback was rebuilt as a render host on alloy's stepped mode
(`lattice/src/render_host.rs`), and the render engine is built like a
test engine: `seed_random` with the harness's fixed seed (`srt render
--seed <n>` for another), the wall frozen (`performance.now()` 0, the
calendar on the fixed epoch plus frame time). Verified with an app that
colors a box by `Math.random()`: two renders byte-identical, `--seed 7`
another color. The dev client in run mode is untouched. Not done, on
purpose: seeding a dev-session reload over the control API (nothing asks
for it; the test harness seeds through its own host).
