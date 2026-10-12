---
title: Speech recognition leaves the tree for a runtime module
description: The in-tree Whisper, VAD and wake word lane (lattice speech feature, whisper-rs, livekit-wakeword and its crates-io patch, the SPEECH build knob, @solidrt/core/speech-recognition) ships nowhere and exists for testing; once speech runs as an external runtime module over the microphone ring, the lane and everything built only for it come out.
created: 2026-10-11
---

# Speech recognition leaves the tree for a runtime module

## Symptom

Speech recognition is in the repo as a dev-only lane: `lattice`'s
`speech` feature (on by default, built with `SPEECH=0` in every release
and CI flow), `lattice/src/speech.rs` and `plugins/speech.rs`, the
`whisper-rs` and `livekit-wakeword` dependencies with the
`[patch.crates-io]` fork for the wake word session fix,
`lattice/tests/speech.rs`, `@solidrt/core/speech-recognition` with its
`speech` global in `types.d.ts`, the `SPEECH` knob in `lattice/Makefile`,
`Makefile.android`, `packages/cli/runtime/runtime.mk` and the three
workflows, and the Android cmake toolchain wrapper added for whisper's
build. None of it reaches a user, every bit of it costs build time, binary
size in dev builds and dependency surface, and its home was never decided.

The platform direction is the opposite: a model runs in a custom runtime
module (docs/runtime, "Native code") over the camera tap and the
microphone ring, as the `vision` demo does with ncnn. Speech is the first
such module to exist outside the tree (sherpa-ncnn beside the detector).

## Shape

Once an external speech module runs on the microphone ring, remove the
lane whole:

- lattice: the `speech` feature and both source files, the two
  dependencies, the crates-io patch in the workspace `Cargo.toml`,
  `tests/speech.rs`, the cmake toolchain wrapper if nothing else needs it.
- Build: the `SPEECH` knob everywhere it is read or passed.
- core: `speech-recognition.ts`, its package export and the `speech`
  global type.
- Notes: what the lane taught stays where it already is (the worker
  pacing, the realtime catch-up, the tract optimization trap in
  okf/notes or the memory of it); a one-page note on the recognizer's
  segmentation constants, if the external module wants them, before the
  file goes.

Nothing in the ring or the test source depends on the lane: `TapSource`
and the task-based event delivery go with it, and the pattern is
documented in the runtime docs for the module that replaces it.

## Done looks like

- `cargo tree -p lattice` shows neither whisper-rs nor livekit-wakeword
  and no crates-io patch; `grep SPEECH` finds nothing; the core package
  has no speech export.
- An app that wants speech imports a `runtime:*` module.

## Open

- When: after the first external speech module exists and has been run
  on a device, so the ring has a consumer before the in-tree one goes.
