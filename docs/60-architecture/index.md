# Architecture

How the native side is put together. Four Rust crates, strictly layered,
each usable without the ones above it.

```
lattice   application shell: hosts a SolidRT app on desktop and mobile
flux      JavaScript runtime: QuickJS, plus plugins that marshal to the cores
alloy     platform and rendering: SDL, Impeller, GL, the rendertree, devices
forge     engine-free cores: fs, http, sqlite, subprocess, p2p, wasm, ffi, ...
```

- **forge** holds domain logic with no scripting engine anywhere in it:
  filesystem, HTTP and fetch, SQLite, subprocesses, peer-to-peer, mDNS,
  sockets, image and SVG decoding, WebAssembly, FFI, isolates, the event
  bus. Any embedder can use it, in any language binding.
- **alloy** owns the window and the pixels: SDL for platform windowing and
  input, Impeller for 2D drawing, GL through glow underneath, the
  rendertree that turns a node graph into frames, and the device side
  (camera, microphone, audio, video playback, gamepads).
- **flux** embeds QuickJS and exposes the layers below it to JavaScript.
  Its plugins are split by what they marshal: `standards_plugins/` for the
  web standards (`fetch`, `console`, timers), `forge_plugins/` for the
  `flux:*` capability modules, and `alloy_plugins/` for the GUI bindings.
- **lattice** combines alloy and flux into the runtime that actually hosts
  an app: window lifecycle, storage, the dev-server connection, and the
  `srt:*` modules an app sees on top of Flux (`srt:render`, `srt:events`,
  `srt:dev`, `srt:app`).

Two rules keep the layering honest, and both are load-bearing rather than
stylistic. Plugins only marshal: a plugin converts arguments and results
between JavaScript and Rust and calls a method on the owning crate, which
is where the logic lives. And the rendertree knows nothing about
JavaScript: it takes and returns native Rust types, so a different engine
could drive it tomorrow.

## From JSX to pixels

1. Your JSX compiles to fine-grained reactive updates. There is no virtual
   DOM and no diff.
2. A signal change writes one property on one node through the flux plugin
   boundary.
3. The rendertree marks what changed and requests a frame.
4. Layout runs where it must, the tree records a display list, and the
   raster side draws it.

The JavaScript runtime and the rendering pipeline are not lockstep: JS work
does not block drawing, and drawing does not block JS.

## Demand-driven rendering

The runtime does not spin a render loop. Frames are requested by whatever
changed, and an app that is not animating draws nothing at all, which is
what makes an idle app cost nothing on a battery.

That choice shapes several pieces:

- **Repaint boundaries.** A marked subtree records its own display list and
  is reused until something inside it changes. Transform, scroll, and
  opacity updates on a boundary are applied when compositing, so they cost
  no re-recording at all.
- **Display list reuse.** An unchanged subtree is never re-recorded.
- **Detached elements.** The `d-*` elements sit outside layout, so
  animating one skips reflow entirely.

## Threads

All GL work lives on one raster thread; nothing else touches the context.
The main thread waits on the SDL event queue and wakes for input, for a
frame request, or for a deadline it set itself (an idle tick, a vsync
fallback), rather than polling. JavaScript, layout and paint run on their
own thread with the engine it owns.

## Platforms

Desktop (Linux x64 and arm64, macOS on Apple silicon, Windows x64) and
Android (arm64 and armv7), from the same source and the same crates.
Platform differences surface as named capabilities rather than as OS
checks, both in the Rust layers and in the JavaScript API.

Per-crate pages, and the longer versions of the cross-cutting stories above,
are not written yet.
