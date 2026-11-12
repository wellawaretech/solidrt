---
title: No headless test can read what the paint walk drew
description: The display list is an opaque renderer type, so a unit test sees counters and caches but never which shape was drawn where, at what width or with which paint, and paint behaviour is asserted only by GL examples that CI compiles and does not run; under cfg(test) the renderer's own recording types keep a log of the ops they forward, so a test runs the real walk headless and asserts the ops (texture draws stay out of reach, a headless walk has no texture to draw).
created: 2026-11-11
completed: 2026-11-11
---

# No headless test can read what the paint walk drew

## Symptom

The alloy unit tests run the real paint walk with no GL context (a
`headless()` context per test file and `text_platform()` in
`alloy/src/tests/`). What they can observe of it is:

- the walk's counters (`boundaries_recorded`, `snapshots_rasterized`, the
  paint-op counts of `counters.rs`),
- the caches the walk leaves on the tree (`RecordingCache`, a snapshot's
  key),
- damage extents and the tree's own state.

What was drawn is not among them. `renderer::DisplayList`,
`DisplayListBuilder` and `Paint` are the renderer's types (aliases of the
Impeller ones in `alloy/src/renderer/mod.rs`), and nothing reads an op or
a paint field back out. `alloy/src/tests/renderer.rs` checks that every op
of the contract records and builds, and no more.

So a claim about the picture has two places to go. A pure helper gets a
unit test (`grid.rs`, a kind's method called by itself). Everything
between the helper and the pixels gets a GL example
(`alloy/examples/pixel_snap.rs`), which needs a GL context, is not a
`cargo test`, and which CI compiles (`cargo check --all-targets`) and does
not run.

What that cost while okf/done/border-width-device-pixels.md was built:

- With the stroke floor switched off in the rect build, all 963 lib tests
  passed. Only the example failed. A direct unit test of the kind's method
  closed that; the walk around it stayed out of reach.
- One link has no headless test at all: that the walk hands a running
  width track to the rect build. Delete the line and the lib suite
  passes. The test beside it says so itself
  (`a_recording_of_a_rect_with_a_running_width_track_is_scale_bound` in
  `alloy/src/tests/pixel_snap.rs` holds the two ends and not the width).
- "A recording made again for a new scale holds the new floor" could be
  asserted as "it re-recorded" and not as what it holds.
- The example passed with a stroke of the wrong whole width until it was
  made to measure one, because looking for partial pixels was all it did.

The same holds for every paint rule on a shape: where a box lands after
the snap, the order of a clip and a scroll, what a hoisted matrix is
applied to.

## Done looks like

A unit test of alloy runs the real paint walk headless and reads back
what it drew:

- every recording op of `alloy/src/renderer/contract.rs`, in order, with
  its arguments: the state ops (save, save layer, backdrop layer, restore,
  translate, scale, transform), the clips with their shape and operation,
  the draws with their geometry,
- the fields of a draw's paint that the rendertree set: draw style, stroke
  width, cap, join, miter, color, blend mode, and whether it carries a
  color source or a filter. A field never set reads as the default the
  contract states,
- a path as its segments, so a dashed outline can be measured,
- a nested list (a replayed recording) as one op with its opacity, which
  can be opened,
- a helper over the ops that applies the saves, restores and transforms,
  so a test asks for a box in device pixels and not for raw ops.

A test can then say "the rect drawn for this node has a stroke width of
1/1.5 under a scale of 1.5" or "this box's device edges are whole", with
no GL context, in `cargo test`.

The four gaps above get their tests, and the walk-level link of the border
note is closed.

The log exists under `cfg(test)` only, so it is alloy's own lib tests that
read it. A production build pays nothing: no log, no extra field, no
branch. The flux gui tests and the lattice overlay link alloy without it.

## What it involves

The renderer seam (okf/done/renderer-seam.md) is what makes it small: the
rendertree draws only through the contract traits of
`alloy/src/renderer/contract.rs`, under the aliases of `renderer/mod.rs`.

The seam also decides where the log sits. The contract is written against
the aliases: its methods take `&super::Paint` and `&super::DisplayList`,
and the Impeller impls name their own newtypes. A wrapper that the aliases
name in a test build therefore does not compile beside `impeller.rs`: the
Impeller builder and renderer impls stop matching their traits, the
newtype fields are private to that module, and `Renderer::draw` takes the
list, so the wrapper would reach the raster half too.

**The lean: the log inside the renderer's own types.**

- A new module under `alloy/src/renderer/` holds the op and paint-field
  types: plain data over the vocabulary, compiled under `cfg(test)`.
- In `impeller.rs` the newtypes gain `cfg(test)` fields: `Paint` a copy
  of the fields set so far, `DisplayListBuilder` the ops, `DisplayList`
  the finished log behind a shared pointer (a list is cloned and crosses
  threads), `Path` its segments.
- Each existing forwarder gains one `cfg(test)` line: the builder's 19
  calls, the paint's 11 setters, the path builder's 6.
- The contract's `DisplayList` gains a `cfg(test)` method that returns
  the log, so a second renderer gets a compile error where it has none.

The aliases do not change, Impeller's recording path is still the one that
runs, `tests/renderer.rs` keeps its meaning, and `impeller.rs` stays the
only module that names `impellers`.

The log is what the walk asked for. A test owns the builder it hands the
walk (`paint_phase` takes it), so it builds the list and reads the log; a
recording is read from the list its `RecordingCache` holds.

Two shapes that were dropped:

- **A wrapper the aliases name in a test build.** It does not compile
  against the contract as written (above). It would need associated types
  on the contract in place of the aliases, which turns over the seam's
  decision of 2026-10-20 for a test facility.
- **A second renderer that only logs.** A module that implements the
  whole contract by recording, selected for the test build. No Impeller in
  the tests at all. It has to implement the raster half of the contract as
  stubs, and the suites then stop exercising the renderer that ships.

The seam plan leaves an own plain-data op list out of scope until
something consumes it, for its cost per op. This is that consumer in a
form that costs a shipped build nothing.

The replacement renderer of okf/notes/replacing-impeller.md will own its
display list, and an own list can be read. If that work is near, this item
is its first consumer: the op types and the tests carry over, and the
`cfg(test)` lines in `impeller.rs` go with Impeller. If it is not, they
pay for themselves on the next paint rule.

## Not part of it

- Texture draws. The log takes one like any other op, but a headless walk
  never emits one: a texture handle comes only from `adopt_texture` on the
  raster thread, and the test context has no raster thread behind it. So
  the quad of a text layer, an image and the quad of a snapshot stay with
  the GL examples, and with them the one rounding a text gets. A handle a
  test can mint, and something to answer the raster calls that make one,
  is larger than this and its own item.
- Pixels. A raster test still needs GL; this reads the ops, one step
  before them. The GL examples stay.
- Running the GL examples in CI. Worth its own look: they are compiled and
  never run there.

## Related

- okf/done/renderer-seam.md: the contract the log hangs on, and the own
  op list it left out of scope.
- okf/notes/replacing-impeller.md: the renderer whose list could be read
  without the log.
- okf/done/border-width-device-pixels.md: where the gap showed, and the
  one link it left untested.
