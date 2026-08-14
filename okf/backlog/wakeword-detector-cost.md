---
title: The armed wake-word detector burns ~40% of a core while idle
description: Every 100ms the armed speech worker scores a 2.2s window with the stateless livekit-wakeword predict, ~35-40ms a check whether or not anyone is speaking; VAD gating cuts idle cost ~30x for ten lines, a streaming detector fixes it properly but needs upstream surgery.
created: 2026-08-14
---

# The armed wake-word detector burns ~40% of a core while idle

Parked, and worth raising with the LiveKit folks first - ask what they plan
upstream before building (b) ourselves.

The armed speech worker ([lattice/src/speech.rs](../../lattice/src/speech.rs))
scores a 2.2s window every 100ms through livekit-wakeword's stateless `predict`.
That is ~35-40ms per check, about 40% of one core on a desktop laptop at full
power, and it costs the same whether the room is silent or not. On a phone that
is battery burned to hear nothing.

## Two fixes, measured or estimated

**(a) Short-tail VAD gating.** Run Silero over only the newest ~0.3s and skip
the detector when the window holds no speech. Measured at ~1.3ms per check at
full power (4ms in power-save), so idle cost drops roughly 30x. About ten lines
in the armed branch of speech.rs. Cheap, local, no upstream dependency.

**(b) Streaming detector.** Keep mel-frame and embedding ring buffers so each
check computes only the ~1.25 new embeddings instead of all 18: ~3ms per check,
speech or silence, which supersedes (a) rather than stacking with it. Needs
surgery inside `vendor/livekit-wakeword` - the mel and embedding stages are
`pub(crate)` and the API is batch-only - plus bit-equivalence tests against the
batch path. This is the natural upstream feature, which is the other reason to
ask before writing it.

Profile on Android before building (b): the ratio between the batch and
streaming paths may look different on a phone, and (a) may be enough there.

## Benchmarking caveat

Measure only at the full power profile. Power-save inflated the VAD numbers
~2.7x and would have made the gating look far less attractive than it is. This
applies to any CPU measurement in this tree, not just this one.

Source: root TODO.md, migrated 2026-08-14.
