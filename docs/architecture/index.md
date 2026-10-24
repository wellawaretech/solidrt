# Architecture

This page shows the structure of the native side. The native side has four
Rust crates in strict layers. You can use each crate without the crates
above it.

```
lattice   application shell: hosts a SolidRT app on desktop and mobile
flux      JavaScript runtime: QuickJS, plus plugins that marshal to the cores
alloy     platform and rendering: SDL, Impeller, GL, the rendertree, devices
forge     engine-free cores: fs, http, sqlite, subprocess, p2p, wasm, ...
```

- **forge** contains domain logic with no scripting engine in it. It has
  the filesystem, HTTP and fetch, SQLite, subprocesses, peer-to-peer, mDNS,
  sockets, image and SVG decoding, WebAssembly, isolates, and the event
  bus. Each embedder can use it, with each language binding.
- **alloy** controls the window and the pixels. It uses SDL for platform
  windows and input, Impeller for 2D drawing, and GL through glow below
  Impeller. It also contains the rendertree, which makes frames from a node
  graph, and the device side (camera, microphone, audio, video playback,
  gamepads).
- **flux** embeds QuickJS and gives JavaScript access to the layers below
  it. Its plugins are in three groups. Each group marshals a different
  layer: `standards_plugins/` for the web standards (`fetch`, `console`,
  timers), `forge_plugins/` for the `flux:*` capability modules, and
  `alloy_plugins/` for the GUI bindings.
- **lattice** combines alloy and flux into the runtime that hosts an app.
  It controls the window lifecycle, storage, the dev-server connection, and
  the `sol:*` modules that an app gets in addition to Flux (`sol:render`,
  `sol:events`, `sol:dev`, `sol:app`).

Two rules keep the layers correct. These rules are necessary for the
structure. They are not only a matter of style.

- Plugins only marshal. A plugin marshals arguments and results between
  JavaScript and Rust. Then it calls a method on the crate that owns the
  logic.
- The rendertree has no JavaScript references. It takes and returns native
  Rust types. Thus, a different engine can also control it.

## From JSX to pixels

1. Your JSX compiles to fine-grained reactive updates. There is no virtual
   DOM and no diff.
2. When a signal changes, it writes one property on one node. The write
   goes through the flux plugin boundary.
3. The rendertree marks the changes and requests a frame.
4. Layout runs only where it is necessary. The tree records a display list,
   and the raster side draws it.

The JavaScript runtime and the rendering pipeline do not operate in
lockstep. Drawing does not wait for JavaScript work, and JavaScript work
does not wait for drawing.

## Demand-driven rendering

The runtime does not use a continuous render loop. A change requests a
frame. An app without animation draws nothing. Thus, an idle app does not
use battery power to draw.

This decision has an effect on these parts:

- **Repaint boundaries.** A marked subtree records its own display list.
  The runtime uses that list again until a change occurs inside the
  subtree. The runtime applies transform, scroll, and opacity changes on a
  boundary during compositing. These changes do not cause a new recording.
- **Display list reuse.** The runtime does not record an unchanged subtree
  again.
- **Detached elements.** The `d-*` elements are outside of layout. An
  animation of one of these elements causes no reflow.

## Threads

All GL work occurs on one raster thread. No other thread uses the GL
context.

The main thread does not poll. It waits on the SDL event queue. It
continues when one of these occurs:

- input,
- a frame request,
- a deadline that it set (an idle tick, a vsync fallback).

JavaScript, layout, and paint run on a different thread. That thread owns
the engine.

## Platforms

SolidRT operates on these platforms, from the same source and the same
crates:

- Desktop: Linux x64 and arm64, macOS on Apple silicon, Windows x64.
- Android: arm64 and armv7.

Platform differences show as named capabilities, not as OS checks. This is
true in the Rust layers and in the JavaScript API.

Pages for each crate are not available yet. Longer descriptions of the
topics above are also not available yet.
