---
title: Move JSX compilation to the native Oxc compiler
description: Solid 2.0 ships @solidjs/compiler (native, ~30x faster than Babel) alongside the Babel plugin, but it only lowers JSX, so adopting it means finding new homes for TypeScript stripping and the binary/text import inlining that live in our Babel pipeline.
created: 2026-08-28
---

# Move JSX compilation to the native Oxc compiler

## Situation

Solid 2.0-rc.3 deleted `babel-preset-solid` and split it into two successors:
`@solidjs/babel-plugin` (the same JSX compiler as a Babel plugin, which we
migrated to) and `@solidjs/compiler` (a native Oxc compiler shipped as
per-platform NAPI binaries, now the default in `vite-plugin-solid`). Upstream
frames the Babel plugin as the JavaScript fallback, so the native compiler is
where the maintained path is heading.

Our JSX compilation is one call in `packages/cli/src/bundle/bundler.ts`
(`solidPlugin`): a Bun `onLoad` hook that runs every app `.js`/`.ts`/`.jsx`/
`.tsx` through Babel with four things stacked in one traversal - the Solid
plugin (`generate: "universal"`, `moduleName: "@solidrt/core"`),
`@babel/preset-typescript`, `@babel/plugin-syntax-jsx`, and our own
`inlineImport` visitor - collecting per-file source maps into `babelMaps`
for later composition through `@jridgewell/remapping`.

## What it would actually buy, measured

Measured locally on this tree, native vs the Babel pipeline it would replace:

| Corpus | Babel (solid + ts) | Native (JSX only) |
| --- | ---: | ---: |
| 80 real `.tsx` (launcher + core examples + components), 278 KB | 391 ms | 11.7 ms |
| Launcher's own 10 `.tsx` | 72 ms | 2.3 ms |

The second row is the one that decides this. A full `srt bundle` of the
launcher is about 1050 ms wall, so Babel's JSX+TS work is roughly 7 percent
of it. Swapping compilers saves about 70 ms on a one second bundle and
changes nothing anybody would notice.

The numbers are not like-for-like: the native column does not strip
TypeScript (see below), so a real swap would add a TS pass back and the gap
would narrow. Upstream reports Babel's per-file cost growing super-linearly
with module size (24 s for a 1 MB module against 70 ms native), which is the
shape that eventually makes this worth doing - a big app, or one generated
file, not the app sizes we bundle today.

## What blocks a straight swap

Three things, all verified rather than assumed:

1. **It does not strip TypeScript.** `transform()` is a JSX lowering pass
   only. Fed our `.tsx`, its output still contains `type Row = {...}`,
   `let rows: Row[]`, `x as string` and `(r: Row) => ...`. Something else has
   to erase types: Bun's own loader could, but then the pipeline is
   native-then-Bun instead of one Babel traversal, and the ordering against
   `inlineImport` has to be re-established.
2. **`inlineImport` is a Babel visitor and cannot simply move.** It rewrites
   `import data from "../x" with { type: "binary" | "text" }` into an inline
   `Uint8Array`/string, and it exists precisely because Bun's bundler and its
   plugins do not surface import attributes in this Bun version. Babel is
   currently the only place in the build where that AST detail is still
   visible. Dropping Babel means finding another place to see it, or keeping
   a Babel pass alive just for it - which would give back most of the win.
3. **Source map composition gains a hop.** Today one Babel map per file feeds
   `remapping`. Native JSX plus a separate TS erase pass is two maps to
   compose before Bun's own, and `@solidjs/compiler` returns its map as a
   JSON string rather than an object.

A fourth, smaller consideration: the compiler ships as six optional
per-platform packages (`darwin-x64`, `darwin-arm64`, `linux-x64-gnu`,
`linux-arm64-gnu`, `win32-x64-msvc`, plus a `wasm32-wasi` fallback). It would
become a native build-time dependency of the published CLI, which today is
pure JavaScript and runs anywhere Bun does. The WASI fallback covers the gap
where no native binding loads, at some speed cost.

## Shapes to weigh

1. **Status quo.** One Babel traversal doing all four jobs. Slowest, but it
   is a single well-understood pass with one source map per file, and the
   cost is not currently visible in bundle wall time.
2. **Native JSX, Bun for TypeScript, `inlineImport` relocated.** The full
   win, and the version upstream is steering toward. Needs an answer for
   import attributes that does not involve Babel: either a targeted pre-pass
   over the source text, or accepting an extra parse.
3. **Native JSX, Babel retained only for files that need it.** Route a file
   to Babel when it carries an import attribute and to the native compiler
   otherwise. Gets most of the speed with no new attribute mechanism, at the
   cost of two compilers that must not diverge. Note that rc.3's changelog
   records fixes for exactly that class of divergence ("Babel and native
   compiler lowering divergences around nested content, custom-element
   ownership, static attributes, namespaces, and conditional evaluation
   order"), so equivalence needs testing, not assuming.

## When to pick this up

Not on current numbers. The trigger is bundle time becoming visible: a much
larger app, a generated or vendored module big enough to hit Babel's
super-linear range, or upstream letting `@solidjs/babel-plugin` lag behind
the native compiler in correctness or features. Until one of those, the
Babel path is the cheaper correct choice.

If it is picked up, the cheap proof that a swap is behaviour-preserving is
the one used for the rc.3 migration: copy the two checked-in bundles
(`lattice/resources/launcher/index.srt.js`, `lattice/resources/bsod/bsod.srt.js`),
rebuild with `make launcher-bundle -B`, and `cmp`. Byte equality there
exercises the real pipeline over real app code. A compiler swap will not be
byte-identical the way the plugin rename was, so the diff has to be read
rather than just compared.
