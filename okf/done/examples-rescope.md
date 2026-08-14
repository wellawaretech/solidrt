---
title: Examples re-scope and tsconfigs
description: "The examples taxonomy settles: packages examples are agent-facing, root examples/ are human-facing apps, scratch moves to a gitignored sandbox, tsconfigs split by target."
created: 2026-07-25
completed: 2026-07-25
---

# Examples re-scope and tsconfigs

## Motivation

Two threads converged in one debugging session (2026-07-25):

1. The dev-server startup typecheck (okf/plans/dev-server-typecheck.md, done)
   surfaced that a whole-project `tsc` run from the repo root reports errors
   in unrelated files: the root `tsconfig.json` has no `include`, so every
   `.ts`/`.tsx` in the repo is a root file of one program. Worse, ambient
   globals leak across that program: `packages/cli`'s
   `import type { BunPlugin } from "bun"` pulls `@types/bun` -> `bun-types`
   -> `@types/node`, whose global `process` declaration then silently types
   `process.env.NODE_ENV` in `packages/core/src/renderer.ts` - a file that
   must never see Bun types. Core is flux-targeted; cli is bun-targeted;
   one shared tsconfig program cannot serve both.
2. Root `examples/` turned out to be two tracked files (grid.tsx,
   recurse.tsx) plus untracked scratch - a personal sandbox wearing a
   documentation folder's name. The website plan (okf/plans/website.md)
   assumes `examples/` is the source for generated example pages; the
   folder as it stands cannot honor that.

## The taxonomy (decided)

Three audiences, three homes:

- **`packages/*/examples/` - agent-facing.** One concept in isolation per
  file: the shape that gives an LLM maximum signal. Ships in the npm
  tarball (already in core's `files`) so agents find them in
  `node_modules` next to `types.d.ts`. NOT website source material.
  `packages/core/examples/` (22 files) already is this;
  `packages/components` should grow one under the same contract.
- **Root `examples/` - human-facing.** Realistic apps where concepts mix
  naturally. Committed, curated, scaffold-shaped (one folder per app with
  `package.json` + tsconfig + `src/` + assets, i.e. what `srt init`
  produces). Consumed by the website examples generator and as
  documentation references. Because a broken example is a broken docs
  page, `srt check` over the corpus is the docs-correctness gate (cheap:
  ~0.1s per app with the entry-scoped check).
- **`sandbox/` - dev-facing, gitignored.** Probes, scratch apps, media,
  in-progress experiments: what most of today's root `examples/` content
  actually is. Serving from it still works and still typechecks (it gets
  a minimal scaffold-shaped tsconfig); git never sees it.

Truly product-scale apps (e.g. `~/solidrt/projects/*`) stay outside the
repo; in-repo example apps are the middle tier - realistic but small
enough to maintain against every API change in the same commit.

**Flat, not layer subfolders.** `examples/<app>/`, no `examples/core/` /
`examples/components/` nesting. The layer an app belongs to (bare core vs
components vs a future framework) is already recorded truthfully in its
`package.json` dependencies; the generator derives section attribution
from that. Location-as-metadata can lie and go stale, punishes apps that
change layers (broken paths/URLs), and collapses once a second
classification axis appears (headless vs gui, camera, p2p). The by-layer
view is the website's job, not the filesystem's. An explicit `solidrt`
package.json field for richer tagging is a named future - not added until
the generator demands it.

## Per-package tsconfigs (decided)

Split by *target environment*, not by folder convenience:

- `packages/core`, `packages/components`, root `examples/` apps, sandbox:
  **flux-targeted** - scaffold-shaped options (`jsx: "preserve"`,
  `jsxImportSource: "@solidrt/core"`, `moduleResolution: "bundler"`,
  `strict`, `skipLibCheck`, `types: ["@solidrt/flux-types"]`). No DOM lib,
  never `@types/bun`. Whether core/components keep the root config's extra
  strictness knobs (`noUncheckedIndexedAccess` etc.) is decided per
  package - they are our code, so keeping them is the lean; example apps
  and sandbox use plain scaffold settings (app parity).
- `packages/cli`: **bun-targeted** - `@types/bun` legitimately belongs.
- Root `tsconfig.json` shrinks to whatever stray root-level files remain,
  or disappears once nothing is left uncovered. Its
  `exclude: ["**/node_modules", "**/target"]` (added 2026-07-25 for the
  CMake fake-.ts files under `lattice/target/**`) stays as long as the
  root config exists.

This makes the bun-types contamination structurally impossible, also in
the IDE's per-file view (nearest tsconfig wins).

## Entry-scoped typecheck (decided, extends dev-server-typecheck.md)

`typecheck()` stops checking the enclosing project and checks the entry's
program instead: generate a transient config in the project-local
`.srt-data/` (the established dev-artifact dir; the proxy cache db already
lives there) with absolute paths -

    { "extends": "<nearest tsconfig>", "files": ["<entry>"] }

- run `tsc -p` on it, delete it. tsc builds the program from the entry's
import closure only: unrelated files are excluded by construction, not by
filtering. Verified 2026-07-25: 0.09-0.18s vs tens of seconds for the
sweep; `types` resolution works from `.srt-data/` (node_modules walk-up).
Placement notes: absolute `extends`/`files` required; the watcher ignores
`.json` so no reload fires. The tsc binary lookup must walk up past a
node_modules-less project root (an `examples/<app>/` has a tsconfig but no
own node_modules). Applies to both the dev-server startup check and
`srt check`.

The speed also reopens per-hot-reload checking (rejected earlier on
full-sweep latency grounds) - a named future, not in scope.

## Small fix folded in (superseded same day)

`packages/core/src/renderer.ts` used `process.env.NODE_ENV`, a
bundler-time `define` substitution, not a runtime global; nothing in
core's type surface declared it (flux-types has only the `flux:process`
module, deliberately no global). It typechecked by accident of the root
sweep (bun-types' global `process` via cli). First fixed with a
file-scoped `declare const process`, then replaced entirely the same day:
core's vocabulary is now `import.meta.env.DEV` (Vite-shaped), defined by
the srt bundler from `opts.dev` and typed once in core's `types.d.ts`
(`declare global { interface ImportMeta ... }`). Survey showed this was
the ONLY fold site in the codebase - the solid deps select dev/prod at
module-resolution time (export conditions) and contain zero NODE_ENV
reads. DCE verified: prod bundle drops the whole sentinel body, dev
bundle keeps it. The `process.env.NODE_ENV` define stays in the bundler
as ecosystem compat only (third-party libs read it; unresolved `process`
crashes at import). The runtime dev/prod signal remains a separate
concern owned by okf/backlog/dev-prod-validation-policy.md - a runtime
value can never fold, so it was wrong for this site anyway.

## Staging (bare minimum first)

1. **tsconfig split + fixes**: per-package tsconfigs (core, components,
   cli), renderer.ts declare, entry-scoped `typecheck()`. This alone ends
   the noise that started the thread. DONE 2026-07-25, verified: `srt
   check examples/grid.tsx` and the dev-server startup check both print
   "Types OK" (entry-scoped, transient config cleaned up); per-package
   programs run in 0.1-0.3s each. Findings: `packages/cli/server/` is the
   FLUX dev server, not bun code - it already had a correct flux-targeted
   tsconfig (nearest-config), it only needed `skipLibCheck` (dependency
   d.ts noise) and exclusion from the cli program (cli includes `src`
   only); a stale cli tsconfig (extends root, include bin+src) was
   replaced; core excludes `**/_*` scratch files. The pre-existing
   errors (core 10, components 10, cli 6) were zeroed later the same
   day, all type-only, no behavior change: text-input `range()`
   annotated as a tuple (the whole 9-diagnostic cluster), pressable's
   resolved children cast to any (the `children()` return type erases
   the render-prop variant), two response-shape casts in cli's
   dev-server.ts, babel typings (`@types/babel__core` devDep in
   packages/cli gives transformAsync/plugin types; the plugin/preset
   packages have no DT coverage by convention and are declared as
   `PluginItem` in `packages/cli/src/untyped-deps.d.ts` - note cli's
   `types: ["bun"]` does not block this, the types field only gates
   auto-loaded globals, import-driven @types resolution still works),
   and `*.wav`/`*.ogg` joined the existing
   binary-asset wildcard declarations in core's runtime-modules.d.ts
   (which already owned `*.png`/`*.jpg`/`*.svg` - the asset-typing
   owner question answered itself). All four programs report zero
   errors; srt check green on hello, sound.tsx, and sandbox/gallery.
2. **Folder split**: create gitignored `sandbox/`, move the untracked
   scratch there (user's working files - coordinate, do not bulk-move
   unasked); re-home grid.tsx (agent example: single concept, visibility
   dedupe) and recurse.tsx into `packages/core/examples/`. DONE
   2026-07-25: sandbox/ created (contents gitignored, only its
   scaffold-shaped tsconfig committed via `sandbox/*` + negation), all
   scratch + media + terminal/ moved there, grid/recurse re-homed (add
   zero errors under core's stricter config), root tsconfig excludes
   sandbox. Reference-updating was dropped: examples restart from
   scratch (see stage 3). Two fixes surfaced by the move: (a)
   `loadAppIdentity` defaulted displayName to the enclosing package name,
   so any entry under a scoped package (`@solidrt/core`) failed its own
   path-separator validation - scoped names now default to their last
   segment (explicit bad config still fails); (b) the entry-scoped
   transient config must set `include: []` - files and an inherited
   include are UNIONED, so extending an include-bearing tsconfig (core's
   `include: ["."]`) silently dragged the whole package back into the
   program. Verified closure-exact after both: grid clean, sound.tsx
   reports only its own blip.wav gap, sandbox/gallery only the
   pressable.tsx closure error.
3. **First example apps**: REFRAMED 2026-07-25 - the corpus is built from
   scratch as documentation-first apps (the old terminal/gallery scratch
   was moved to sandbox/, not promoted; nothing in `examples/` is
   inherited). Each app: scaffold-shaped folder, `srt check` green.
   First app landed 2026-07-25: `examples/hello/` (core-only hello
   world, root view padded with all four reactive safeArea() insets;
   srt check green). Naming rule: clean names, no layer suffixes - an
   unsuffixed name is core by default, deps carry the real attribution.
4. **Corpus growth + CI**: components agent-examples folder; `srt check`
   across `examples/*` as a CI gate; website generator stage 2 consumes
   the corpus (that work lives in website.md). CI half DONE 2026-07-25:
   ci.yml gained a `types` job (parallel to the Rust check, bun-only;
   runs on PRs AND every push to main, while the Rust job stays PR-only -
   direct-to-main is the working flow, so the push trigger is what makes
   the gate real; quick-check stays native-builds-only by decision) -
   tsc over the five per-target tsconfigs (core, components, cli,
   cli/server, lattice/launcher) plus `srt check` on every
   `examples/*/src/index.tsx` (the corpus entry convention). The
   lattice/launcher straggler got its flux-targeted tsconfig in the same
   pass (one TS2532 fixed: split()[0] non-null) and the root tsconfig now
   excludes lattice too - it covers only strays. Verified locally with
   the exact CI commands, all green. Remaining in this stage: corpus
   content (more apps), components agent examples (seed exists:
   examples/theme-toggle.tsx), website generator.

## Named futures

- ~~Asset-import typings~~ resolved 2026-07-25: core's
  runtime-modules.d.ts already owned the binary-asset wildcard
  declarations (`*.png`/`*.jpg`/`*.svg`); `*.wav` and `*.ogg` joined
  them. Further extensions join the same list as they come into use.

- Templates generated from living example apps at release time (scaffold
  template copies go stale - the gallery already migrated out once;
  release.yml's publish-time rewriting is the precedent).
- Per-hot-reload typecheck now that entry-scoped runs are ~0.1s.
- Explicit `solidrt` package.json tagging for examples if deps-derived
  attribution ever falls short.
- Runtime (flux) examples: `flux/examples/*.js` are untracked scratch
  today; the same agent/human split presumably applies when the Runtime
  docs section needs sources.
