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
  `tests/speech.rs`.
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

## Closed 2026-10-13

Speech runs as a custom runtime module over the microphone ring, so the
lane came out whole: the lattice feature, both source files, the test, the
two dependencies and the crates-io fork, the `SPEECH` knob in
`runtime.mk`, `lattice/Makefile` (with the `DIST` switch, which existed
only to force it off), `Makefile.android` and the three workflows, the
`whisper_rs` branch of alloy's log filter, `@solidrt/core/speech-recognition`
with its export and the `speech` global type, and the mentions in
docs/runtime and the core docs. [wakeword-detector-cost](wakeword-detector-cost.md)
closed with it.

Three things looked speech-only and stay, with their comments made general:

- `packages/cli/runtime/android-abi.toolchain.cmake`, added for
  whisper-rs-sys: it forces 16 KB page alignment on every cmake-built `.so`
  (libSDL3 included), and its ABI default is what lets an out-of-tree module
  build whisper-rs-sys for Android.
- The MSVC runtime-library env vars in `runtime.mk`
  (`CMAKE_POLICY_DEFAULT_CMP0091`, `CMAKE_MSVC_RUNTIME_LIBRARY`): any crate
  that runs its own cmake project needs them, a custom runtime building
  whisper on Windows included.
- The `libc++_shared.so` staging on Android: it applies to any C++ module.

The microphone ring and tap, the session id, `install_test_microphone` and
`app.microphone` are the ring's, not the lane's, and the external module is
built on them.

The recognizer's segmentation constants, for a module that wants a starting
point (all at 16 kHz mono f32, Whisper's rate): a VAD check at most every
100 ms of new audio and only once 0.5 s is buffered; a 1 s pre-roll kept
while idle so an utterance includes its onset; 1.5 s inspected for trailing
silence while speaking, 700 ms of it ending the utterance, 200 ms kept after
the last speech when trimming; a forced cut at 30 s; interim re-transcription
at most once per 1 s of new audio; a 2.2 s rolling window for the wake word
detector, which needs about 1.96 s to produce its full embedding sequence
and silently scores zero on less.
