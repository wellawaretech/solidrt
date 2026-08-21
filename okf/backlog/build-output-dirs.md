---
title: One build-output root with per-flow subdirs
description: Give dev, render and pack one gitignored output root (dist/) with a subdir per flow, fixing render's missing isolate support and clearing the ground for pack formats and asset pre-processing.
created: 2026-08-21
---

# One build-output root with per-flow subdirs

## Symptom

`srt render` does not support isolates. The bundler builds one bundle per
"use isolate" module, but only the dev-server path writes them to disk
(`.srt-data/isolates/`); render writes just the main bundle and passes the
project root as `--assets`, so the runtime's `isolates/<id>.js` lookup finds
nothing and `isolate(id)` rejects with "no such isolate module in this app".

Underneath it: build outputs are scattered. Render drops `<entry>.srt.js`
next to the source, `pack --folder` dumps runner + manifest + assets straight
into `dist/` root, the single-file exe lands next to the source, and dev
isolate bundles live in `.srt-data/`. There is no place a flow can stage a
complete app-shaped directory.

## Decision

`dist/` is the build root, in the spirit of Cargo's `target/`: one gitignored
directory (already in the scaffold gitignore and the bundler's SKIP_DIRS),
one subdir per flow, and each flow owns - and may wipe - only its own subdir:

```
dist/
  render/    staged run dir: <name>.srt.js + isolates/<id>.js + assets/
  pack/      the canonical pack folder (current --folder output, moved down)
  <format>/  future pack formats, one subdir each (e.g. steam/)
```

- **render** builds into `dist/render/` and passes that dir as `--assets`.
  This is the same shape as an installed version dir (`assets/` plus
  `isolates/` under one root, what `AssetsBase::Dir` expects), so no runtime
  or forge changes - and it fixes both the isolate gap and the source-adjacent
  `.srt.js` wart. Assets are copied in (reuse `collectAssets`, as pack does);
  the subdir is wiped first so removed isolates and deleted assets cannot go
  stale.
- **pack --folder** defaults to `dist/pack/` instead of `dist/`. Pack will
  grow more formats (a Steam depot layout, for instance); the root must not
  be any single format's output or the first new format collides. The
  single-file exe stays a deliverable: `--output`, default next to the
  source, like `cargo install` vs `target/`.
- **dev stays in `.srt-data/`** for now. Its isolate bundles are dev-server
  serving state tied to a running server, closer to runtime data than build
  output. Revisit when asset pre-processing exists (below).

## Why this shape also carries asset pre-processing

Once flows read assets from a staged `dist/<flow>/assets/` instead of the raw
project tree, the copy step is a pipeline stage: image minimization, SVG
minification, atlasing slot in there and the runtime never knows - the
contract stays "a dir containing assets/". Consequences, deferred but known:

- Incrementality becomes mandatory at that point (plain copy can be dumb,
  re-encoding cannot): a hash/mtime manifest per subdir deciding what to
  reprocess. The wipe-own-subdir rule is what invalidates it.
- One shared "build assets into <dir>" step used by every flow, deterministic,
  parameterized by profile if formats ever diverge.
- Dev is the odd one out (it resolves assets live from the project root).
  When a transform actually exists, dev either processes on the fly in the
  dev server's asset route or folds into the same staging with a watcher.
  Until then it stays a raw mount.

## State

Landed 2026-08-21: `srt render` stages `dist/render/` (bundle + isolates/ +
copied assets) and passes it as `--assets`; verified by
probes/isolate-render-probe.tsx (frame and log show the isolate reply).
`pack --folder` defaults to `dist/pack/`. Render no longer writes `.srt.js`
next to sources.

Also landed 2026-08-21: `srt bundle` no longer drops isolate bundles, and is
a directory flow like the others. Output goes to `dist/bundle/` (or
`--output <dir>`, refused when the dir is non-empty and not a previous
bundle output - the writePackFolder rule): the bundle plus `isolates/<id>.js`
(`.bin` with --compile). The `.js` and `.bin` forms share the isolates/ dir,
so a rebuild clears only the form it rewrites. Prebuilt `.srt.js` loads
(server startup, repl `load`) read the sibling isolates/ dir back and
republish through the dev flow; `--stdout` warns that it carries only the
main bundle.

`srt bundle --flux` carries isolate modules too. Standalone flux resolves by
location, not directive - the contract moved from `<entry dir>/<id>.js` to
`<entry dir>/isolates/<id>.js` (flux.rs resolver; flux/examples/isolates/) -
so bundling preserves the shape: everything under the entry's isolates/ dir
is built bare and lands as isolates/<id>.js next to the bundle, and a
bundled flux script runs from dist/bundle/ unchanged. A worker may be .ts
when bundling (built to .js), unlike running from source.

Bugs fixed en route: `isSource` matched any `.js` so the server re-bundled a
prebuilt `.srt.js` as source (prebuilt branch was dead code).

The flux bytecode/pack gaps are closed (okf/done/flux-packed-isolates.md):
the flux resolver reads .bin first, `--flux --compile` emits isolate
bytecode, and packed flux exes carry and resolve isolates via the shared
section trailer. Known gap left: repl `load x.srt.bin` pushes bytecode
without a manifest, so isolates cannot travel on that path.

Remaining here: the asset pre-processing pipeline above (with its
incrementality manifest) and folding dev into the same staging - both waiting
on an actual transform to exist.
