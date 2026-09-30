---
title: A headless render is not reproducible when the app draws random numbers
description: srt render and playback step time deterministically but leave Math.random on the engine's clock-seeded generator, so an app with particles, a shake or a shuffled list renders other frames on every run; seed the context with flux's seed_random, fixed by default, with a --seed option.
created: 2026-09-30
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
