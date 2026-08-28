---
title: Demos a user can run, shipped inside the packages
description: Done 2026-08-25 - a package's demos/ folder is one project (shared package.json, tsconfig and assets/, one src/*.tsx per demo); srt demo lists them numbered and runs one by number or qualified name, by starting the ordinary dev server with its cwd set to that project.
tags: [cli, 3d, demos]
created: 2026-08-25
completed: 2026-08-25
---

# Demos a user can run

Every package already had `examples/`, written for an LLM: one file per
feature, "copy one and adapt it". Missing was the other audience - a demo a
person runs to see what the package does. "The third dimension" is the case
that started it, and it needs @solidrt/3d, so it belongs in that package
rather than in a demos folder at the repo root.

## What shipped

`srt demo` lists the demos the installed `@solidrt/*` packages ship,
numbered:

```
  1  3d/the-third-dimension
```

`srt demo 1` runs one; the qualified `<package>/<demo>` name works too, so a
script can name one without depending on the numbering. Index and name are
the whole listing - no per-demo description, because a description in the
listing is a second place to keep the demo's story.

A package's `demos/` folder is ONE project: `package.json`, `tsconfig.json`,
one `assets/`, and `src/<name>.tsx` per demo, with `demos/` added to the
package's `files`. The first is `packages/3d/demos/`, holding
`the-third-dimension.tsx` - the standalone demo project moved in, with its
`shaders.ts` folded into the same file (which let `GROUND_FRAGMENT`
interpolate `FLOOR_SIZE` instead of restating it under a keep-in-sync
comment). Its icon is the puzzle mark.

`packages/*/demos/src/*.tsx` joined `CHECK_ALL_GLOBS`, so a repo-root
`srt check` typechecks every demo and they cannot rot quietly.

## Why a project per package, not a file per demo

The first shape was one self-contained `.tsx` per demo, discovered by glob
and run in `--file` mode. That is smaller, but file mode has no `/assets/`
route, so no demo could ever load a texture, a clip or a font - and a 3D demo
that may not ship art is a demo with one hand tied. Making `demos/` a project
buys the assets folder and costs one package.json.

The payoff is that `srt demo` needs almost no machinery of its own.
`src/demo/main.ts` only lists and resolves; `main.ts` then calls the same
`launchServer` that `run` uses, with the spawn's `cwd` set to the demos
project and `--project` passed. `resolveMode` decides the project from the
cwd (never from the entry's nearest package.json), so from there every
downstream part - mode, assets, registry, port memory, reload on save, the
client - behaves as it does for any app, and nothing downstream knows demos
exist.

Discovery reads `node_modules/@solidrt/*/demos/src/` with `readdir`, not
`Bun.Glob`: Glob does not follow symlinks, and in a checkout every
`node_modules/@solidrt/*` is a workspace symlink, so a glob finds nothing.
It looks in the cwd and nowhere above it - the same never-search-upward rule
`server/mode.ts` states for the project.

## Accepted consequences

- Two demos of the same package cannot run at once: the server key is the
  project directory, so the second start hits the existing "a dev server
  already serves ..." failure. Demos from different packages run side by
  side. Correct behaviour rather than a limitation - one project, one server.
- `.srt-data` (build outputs, proxy cache) lands inside the demos project,
  which for an installed user is inside `node_modules`. It is a cache,
  writable, and wiped by a reinstall. The alternative - copying the demos
  project to `~/.solidrt/` on first run - buys a copy the user can edit and a
  copy that goes stale, and was not worth it.
- `--port` is not accepted for `demo`, and neither is `--file`/`--project`.
- No demo menu. `srt demo 3d` lists that package's demos; it does not launch
  a picker that navigates between them, which would mean in-app routing
  across demos that deliberately do not share a package boundary.
- Identity is per project, so the launcher shows a demo under the project's
  `displayName`; see okf/backlog/demo-identity-per-demo.md.

## Rules the demos folders keep

One file per demo (shaders included), and one package per demo - a demo uses
its own package plus @solidrt/core and nothing else, so it shows what that
package is for on its own. Both are stated in each `demos/README.md` and are
conventions, not enforcement: a reviewer catches them for free, and an import
check in the bundler would be machinery for one rule.

## Distributed with the CLI, pre-bundled (2026-08-28)

The demos no longer ship as source inside their packages (`demos/` left the
packages' `files`). The CLI carries them instead: `make -C packages/cli
demos` (a step of the release workflow, next to the console pack) writes
`packages/cli/dist/demos/<package>/` - the demos project's package.json and
`assets/`, plus `<slug>/<slug>.srt.js` per demo from `srt bundle --project`
(a dir per demo, because a bundle run owns its output's `isolates/`). So
every CLI install has every demo, without installing the packages, and a
user with only `@solidrt/cli` sees what `@solidrt/3d` can do.

The run path did not change: `srt demo` discovers `dist/demos/` relative to
the CLI instead of `node_modules/@solidrt/*/demos/`, and hands the same
`{ cwd, entry }` to the same `launchServer`. The dev server already served a
prebuilt `.srt.js` entry as-is (no rebuild, no typecheck), so a demo is
still a dev server plus a client, in the registry - the console picks it up
like any app, with tree, stats and control API. That was the point of not
packing demos as `.srtapp` runtime apps: a sealed app shows the picture, a
dev-served one shows the tooling around it too.

Given up: reload on save under `srt demo` (the bundle is what runs; a
checkout works on a demo with `srt run src/<name>.tsx --project` from the
package's `demos/` and rebuilds), and the demos build being optional - a
checkout without `dist/demos/` gets "Demos not built: run make -C
packages/cli demos", the same trade `srt console` makes. `.srt-data` now
lands in `dist/demos/<package>/` inside the CLI package, as it did inside
`node_modules` before. Identity is still per project
(okf/backlog/demo-identity-per-demo.md applies unchanged).
