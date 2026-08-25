---
title: Give srt one folder per command, split by runtime
description: Restructure packages/cli so every command is a top-level folder whose first line says its runtime (bun for node-ecosystem adapters, flux for the dev server), bin/srt only routes, and a server is one process the console can spawn or embed directly.
tags: [cli, flux, bun, dev-server, console, mcp, packaging]
created: 2026-08-25
---

# Give srt one folder per command, split by runtime

Follows okf/backlog/cli-flux-migration.md, which moved the dev server into a
flux process but left srt itself a bun program that launches it. This note
records the shape decided on 2026-08-25 for the next session; nothing in it is
started.

## The problem

`packages/cli` is two programs (srt on bun in `src/`, the dev server on flux
in `server/`) plus two bun helpers the server spawns (`src/entries/`), and
the file names never say which is which: `bundler.ts` bundles the app,
`server-bundle.ts` bundles the server script, `commands/server.ts` launches
the server, `entries/` are not entries but spawned subprocesses. Every
"where is X" question needs the process tree explained first:

```
bin/srt (bun) -> flux server/main.js -> bun entries/bundle-cli.ts    (per rebuild)
                                     -> bun entries/typecheck-cli.ts (at start)
                                     -> solidrt-go                    (srt run)
```

The console (a solidrt app, so flux) needs to spawn servers and clients and
drive them over the control API. Through today's shape that is
console -> bun -> flux -> bun. A server must be one process, spawned directly.

## The rule

Two things in the package are runtime-bound, everything else is not:

- app bundling and typechecking need bun (`Bun.build` + the Solid babel
  plugin, the project's `tsc`);
- the p2p tunnel needs flux (`flux:p2p`).

So: **bun for what the node ecosystem provides (build, typecheck, scaffold,
MCP); flux for everything solidrt does itself (serve, spawn, pack, control).**
The server is flux for the tunnel and because the console and inspector are
flux: server code they can import and run in-process, or spawn as one
process. Bytecode is a bundle output form: `bundle --compile` spawns `fluxc`
(a native binary, any runtime can spawn it); `pack` asks `bundle` for
bytecode and never mentions `fluxc`.

## The shape

```
packages/cli/
  Makefile            dist/server.js (release prebuild of the flux server), nothing else yet
  README.md           the overview: what srt is, the command list, a link per command's docs.md
  AGENTS.md           the agent quickstart, a link per command's agents.md
  package.json  tsconfig.json (bun folders + types/)  server/tsconfig.json (server/ + types/)
  bin/srt             bun. Parses the command word only:
                        init|mcp|check|bundle|pack|render|android|client -> import ./<cmd>/main.ts
                        server|run                                      -> exec flux server/main.ts
                                                                           (env: platform dir, bun path)
  init/     main.ts  scaffold/  docs.md          bun   scaffold + bun install (prompts)
  mcp/      main.ts  docs.md  agents.md          bun   stdio MCP bridge over the control API
  check/    main.ts  docs.md                     bun   bundle in memory (imports bundle/) + tsc
  bundle/   main.ts  docs.md  agents.md          bun   Bun.build + Solid plugin, isolates, sourcemaps;
                                                       --compile via fluxc; --json for the server
  pack/     main.ts  layout.ts  trailer.ts  docs.md  agents.md
                                                 bun   imports bundle/ (compiled), then layout + trailer
  render/   main.ts  docs.md                     bun   imports bundle/, stages dist/render, spawns solidrt-go --playback
  android/  main.ts  docs.md                     bun   adb: device, APK install, launch against a server
  client/   main.ts  docs.md                     bun   spawn solidrt-go against a server (registry / --port / --server)
  server/   main.ts control.ts rebuild.ts logs.ts tunnel.ts qr.ts cache.ts proxy.ts registry.ts state.ts
            docs.md  agents.md                   flux  the dev server, in-process. run = server --client.
                                                       Rebuild spawns `bun bundle/main.ts --json`,
                                                       startup typecheck spawns `bun check/main.ts`.
  lib/                                           bun   what the bun commands share: args parsing, mode
                                                       (project vs file, the key), project config +
                                                       manifest + assets, fonts, artifacts, registry read
  types/    bundle.d.ts  control.d.ts            none  the contracts between the runtimes, .d.ts only
```

Process trees under this shape:

```
srt run      bin/srt -> flux server (in-process) -> solidrt-go
                                                 -> bun check/main.ts      (once)
                                                 -> bun bundle/main.ts     (per reload)
console      flux server ...                      (one process; or imported and run in-process)
srt pack     bin/srt -> pack (bun) -> fluxc
```

## Decisions taken with the shape

- **The server takes flags, not a config blob**: `flux server/main.ts
  [file] [--project|--file] [--port N] [--lan] [--client] [--size WxH]
  [--stats] [--capture f] [--tunnel] [--proxy-http] [-- args]`, resolving
  mode and binaries itself (platform dir from the environment, SRT_HOME for
  checkouts). That is the console's interface too. `shared/config.ts` and
  the JSON handoff go away.
- **Mode resolution lives twice**: `lib/mode.ts` (bun) and in `server/`
  (flux). About 60 lines over exists + realpath + readText. Two honest
  copies over an fs shim; the rule "the key is the realpath of the project
  root or of the file" is stated in one place and referenced by the other.
- **Servers spawned by the console are detached**: a server's lifetime
  belongs to the registry, not to whoever started it.
- **`scaffold/` moves into `init/`.** `docs/` and `agents/` dissolve into
  per-command `docs.md` / `agents.md`: today's `agents/debugging.md` splits
  between `mcp/` and `server/` (the control API without MCP), `agents/assets.md`
  goes to `pack/` (identity, fonts, distribution) with the asset facts in
  `bundle/`. `README.md` is the single overview; `docs/index.md` goes.
  The website mounts `README.md` + `*/docs.md` instead of the `docs/` folder
  (`website/src/build.ts`). `package.json` `files` and the root `CLAUDE.md`
  pointer to `agents/debugging.md` follow.
- **`scripts/build-server.ts` becomes a Makefile target**:
  `bun build server/main.ts --target=browser --format=esm --external 'flux:*'
  --outfile dist/server.js` (verify the CLI accepts the `flux:*` pattern);
  release.yml calls `make -C packages/cli dist/server.js`. `server-bundle.ts`
  goes. Whether the flux server needs a single-file bundle at all is open
  (flux runs one plain JS file today).
- **`types/` is `.d.ts` only**, included by both tsconfigs.

## Flux gaps to close first

Each small and useful on its own, needed by `server/` to resolve its mode
and register itself without bun in front:

- realpath in `flux:fs` (registry keys are canonical paths)
- a pid liveness probe in `flux:process` (`kill(pid)` sends; the registry
  needs "is it alive")
- network interface listing (the `--lan` address; today srt computes it in
  bun and passes it down)
- tty stdin (okf/backlog/stdin-tty-support.md): not needed for this shape,
  since `init` and `mcp` stay bun

## What carries over from the interim cleanup (2026-08-25, uncommitted)

Done in `src/` before this shape was decided and still valid: the single
`solidrt.*` loader (`loadProject`) and its validation, `CLI_VERSION` as the
one version source (`--version`, the manifest stamp, the MCP server), `--help`
and the version banner, the bundle prebuilt contract (`.srt.js` compiles,
`--output` honored, `.srt.bin` rejected), `fail()` in one place, the exe
trailer format and the pack layout as separate files, and the control API
response types. `shared/config.ts` and `shared/registry.ts` are interim and
go with the config blob. The `build/` + `dev-server/` rename considered on
the way is not done: it would tidy the interim shape only to discard it.

## Staging

1. Close the three flux gaps.
2. `server/` on its own: flags instead of the config blob, mode resolved
   in-process, registry written in-process, spawn `bun bundle --json` and
   `bun check` by command name. `srt run`/`srt server` in `bin/srt` become
   an exec. The console can spawn a server from here on.
3. Fold `src/` into the command folders and `lib/`; `bin/srt` becomes the
   router; `scaffold/`, `docs/`, `agents/`, `scripts/` dissolve as above;
   website build, `files`, `CLAUDE.md` pointers follow.
4. Prebuilt `dist/server.js` via the Makefile at release.

Done looks like: `packages/cli` has no folder whose runtime you have to ask
about, `bin/srt` is a router, and `flux server/main.ts` started from any cwd
is a complete dev server.
