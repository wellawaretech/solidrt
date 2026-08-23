# @solidrt/flux-types

TypeScript type definitions for the [SolidRT](https://github.com/wellawaretech/solidrt)
`flux` runtime: the `Flux` global, the `flux:*` capability modules, the
web-standard globals flux provides, and the GUI globals.

The runtime is QuickJS, not a browser or Node, so it ships neither `lib.dom` nor
the Bun/Node libraries. This package is the single source of truth for everything
the flux runtime exposes, including web standards like `fetch`, `Response`,
`WebSocket`, `console`, and timers.

## Installation

```sh
bun add -d @solidrt/flux-types
```

Then configure `tsconfig.json` so TypeScript uses these types and does **not**
pull in browser or Bun/Node globals that the runtime does not have:

```json
{
  "compilerOptions": {
    "lib": ["ESNext"],
    "types": ["@solidrt/flux-types"]
  }
}
```

- `lib: ["ESNext"]` keeps the genuine ECMAScript types (Promise, Array, Map,
  TypedArrays, `Symbol.asyncIterator`) while dropping the DOM. Including `"dom"`
  would advertise `document`, `window`, `localStorage`, and other APIs that do
  not exist in flux.
- `types: ["@solidrt/flux-types"]` makes the `flux:*` modules (ambient module
  declarations) and the runtime globals visible.

Do **not** also add `@types/bun` or `@types/node`: they declare `Bun.*`,
`bun:*`/`node:*` modules, `process`, `Buffer`, and other APIs the runtime does
not provide, and they collide with the web-standard globals declared here.

If you already maintain a `types` array, add to it rather than replacing it:
listing `types` disables TypeScript's automatic inclusion, so every package you
rely on must be named.

## What's covered

- `Flux` global (`version`, `capabilities`).
- `flux:*` modules: `flux:http`, `flux:fs`, `flux:sqlite`, `flux:subprocess`,
  `flux:p2p`, `flux:net`, `flux:mdns`, `flux:process`, `flux:path`, `flux:wasm`, `flux:ffi`,
  and (on a gui-enabled runtime)
  `flux:camera`, `flux:microphone`, `flux:audio`, `flux:gpu`, `flux:spatial`.
- Web-standard globals: `console`, `fetch` + `Headers`/`Request`/`Response`,
  `setTimeout`/`setInterval`/`queueMicrotask`, `performance`, `WebSocket`,
  `TextEncoder`/`TextDecoder`. These are deliberate subsets matching exactly what
  the runtime implements.
- GUI globals (gui-enabled runtime only): `requestAnimationFrame` /
  `cancelAnimationFrame` (web-standard names, so kept global).

## License

MIT. Copyright (c) 2026 Antoine van Wel.