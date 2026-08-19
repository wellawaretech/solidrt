# @solidrt/components

A collection of components for [SolidRT](https://github.com/wellawaretech/solidrt) apps, built on the `@solidrt/core` primitives. Optional: an app can be built with core primitives alone, and a component is just a function returning core elements, so you can always drop down underneath.

> LLM agents: see [AGENTS.md](./AGENTS.md) for a dense, self-contained quickstart.

## Installation

```sh
bun add @solidrt/components   # peers: @solidrt/core, @solidjs/signals
```

Per-component prose lives in `docs/`, one file per module; the props are the typed, commented interfaces in `src/` (this package ships its source, so your editor shows them on hover). The README is generated from both.
