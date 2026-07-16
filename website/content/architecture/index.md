# Architecture

How the native side is put together. Four Rust crates, strictly layered:

- **forge** - engine-free cores: filesystem, HTTP, sqlite, subprocess, p2p,
  events. No scripting engine types anywhere; usable from any embedder.
- **alloy** - rendering: SDL, Impeller, and GL under one roof, with the
  rendertree that turns a node graph into frames.
- **flux** - the JavaScript runtime: QuickJS embedding plus thin plugin
  layers that marshal between JS and the forge/alloy cores.
- **lattice** - the application shell: combines alloy and flux into the
  runtime that hosts SolidRT apps on desktop and mobile.

The rule that keeps it honest: domain logic lives in the owning crate,
plugins only marshal. The rendertree knows nothing about JavaScript.

## Planned here

One page per crate, plus the cross-cutting stories: demand-driven
rendering, repaint boundaries, the event model.
