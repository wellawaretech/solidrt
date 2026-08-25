---
title: Give srt one folder per command, split by runtime
description: Restructure packages/cli so every command is a top-level folder whose first line says its runtime (bun for node-ecosystem adapters, flux for the dev server), bin/srt only routes, and a server is one process the console can spawn or embed directly.
tags: [cli, flux, bun, dev-server, console, mcp, packaging]
created: 2026-08-25
---

# Give srt one folder per command, split by runtime

Follows okf/backlog/cli-flux-migration.md, which moved the dev server into a
flux process but left srt itself a bun program that launches it. This note
records the shape decided on 2026-08-25 and how it was built the same day
(see Staging for where the built shape departs from the diagram).

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
  package.json  tsconfig.json (the bun folders + src/types/)  src/server/tsconfig.json (server/ + types/)
  bin/srt             bun. Parses the command word only:
                        init|mcp|check|bundle|pack|render|android|client -> import ../src/<cmd>/main.ts
                        server|run                                      -> exec flux src/server/main.ts
                                                                           (env: platform dir, bun, cli root)
  src/
    init/     main.ts  scaffold/  docs.md        bun   scaffold + bun install (prompts)
    mcp/      main.ts  docs.md  agents.md        bun   stdio MCP bridge over the control API
    check/    main.ts  docs.md                   bun   bundle in memory (imports bundle/) + tsc
    bundle/   main.ts  docs.md  agents.md        bun   Bun.build + Solid plugin, isolates, sourcemaps;
                                                       --compile via fluxc; --json for the server
    pack/     main.ts  layout.ts  trailer.ts  docs.md  agents.md
                                               bun   imports bundle/ (compiled), then layout + trailer
    render/   main.ts  docs.md                   bun   imports bundle/, stages dist/render, spawns solidrt-go --playback
    android/  main.ts  docs.md                   bun   adb: device, APK install, launch against a server
    client/   main.ts  docs.md                   bun   spawn solidrt-go against a server (registry / --port / --server)
    server/   main.ts args.ts mode.ts binaries.ts registry.ts control.ts rebuild.ts remap.ts
              tunnel.ts qr.ts cache.ts proxy.ts state.ts config.ts docs.md agents.md tsconfig.json
                                                 flux  the dev server, in-process. run = server --client N.
                                                       Rebuild spawns `bun bundle/main.ts --json`,
                                                       startup typecheck spawns `bun check/main.ts`.
    lib/                                         bun   what the bun commands share: args parsing, mode
                                                       (project vs file, the key), project config +
                                                       manifest + assets, fonts, artifacts, registry read
    types/    bundle.d.ts  control.d.ts  registry.d.ts
                                                 none  the contracts between the runtimes, .d.ts only
```

Every folder sits under `src/`: one folder per command, `lib/` and `types/`
beside them, `server/` the one flux folder among bun folders (its own
tsconfig marks it). Nothing but `bin/`, `Makefile`, the docs and `package.json`
lives at the package root.

Process trees under this shape:

```
srt run      bin/srt -> flux src/server (in-process) -> solidrt-go
                                                     -> bun src/check/main.ts   (once)
                                                     -> bun src/bundle/main.ts  (per reload)
console      flux server ...                      (one process; or imported and run in-process)
srt pack     bin/srt -> pack (bun) -> fluxc
```

## Decisions taken with the shape

- **The server takes flags, not a config blob**: `flux src/server/main.ts
  [file] [--project|--file] [--port N] [--lan] [--client] [--size WxH]
  [--stats] [--capture f] [--tunnel] [--proxy-http] [-- args]`, resolving
  mode and binaries itself (platform dir from the environment, SRT_HOME for
  checkouts). That is the console's interface too. `shared/config.ts` and
  the JSON handoff go away.
- **Mode resolution lives twice**: `src/lib/mode.ts` (bun) and `src/server/mode.ts`
  (flux). About 60 lines over exists + realpath + readText. Two honest
  copies over an fs shim; the rule "the key is the realpath of the project
  root or of the file" is stated in one place and referenced by the other.
- **Servers spawned by the console are detached**: a server's lifetime
  belongs to the registry, not to whoever started it.
- **`scaffold/` moves into `src/init/`.** `docs/` and `agents/` dissolve into
  per-command `docs.md` / `agents.md`: today's `agents/debugging.md` splits
  between `mcp/` and `server/` (the control API without MCP), `agents/assets.md`
  goes to `pack/` (identity, fonts, distribution) with the asset facts in
  `bundle/`. `README.md` is the single overview; `docs/index.md` goes.
  The website mounts `README.md` + `*/docs.md` instead of the `docs/` folder
  (`website/src/build.ts`). `package.json` `files` and the root `CLAUDE.md`
  pointer to `agents/debugging.md` follow.
- **`scripts/build-server.ts` becomes a Makefile target**:
  `bun build src/server/main.ts --target=browser --format=esm --external 'flux:*'
  --outfile dist/server.js` (verify the CLI accepts the `flux:*` pattern);
  release.yml calls `make -C packages/cli dist/server.js`. `server-bundle.ts`
  goes. The flux server stays a single-file bundle: flux runs one plain JS
  file, and a checkout builds it per launch (`src/lib/server-bundle.ts`).
- **`src/types/` is `.d.ts` only**, included by both tsconfigs.

## Flux gaps (closed)

Each small and useful on its own, needed by `server/` to resolve its mode
and register itself without bun in front:

- `realpath(path)` in `flux:fs` (registry keys are canonical paths)
- `alive(pid)` in `flux:process` (`kill(pid)` sends; the registry needs "is
  it alive"; a zombie counts as gone)
- `env` in `flux:process`, a snapshot object: flux has no `import.meta`, so
  the server learns where the platform binaries, bun and srt are from the
  environment (below)
- network interface listing: `flux:net` `interfaces()` already existed
- tty stdin (okf/backlog/stdin-tty-support.md): not needed for this shape,
  since `init` and `mcp` stay bun

## The server's interface (stage 2, as built)

```
flux server.js [file] [--project|--file] [--port N] [--lan] [--proxy-http]
               [--capture f] [--tunnel] [--stats] [--minify]
               [--client N [--data-root d] [--size WxH]] [-- args]
```

`--client N` spawns the local client with data slot N (`srt run`, default
0); without it the server runs alone (`srt server`). The environment names
what it spawns: `SRT_PLATFORM_DIR` (the platform binaries), `SRT_CLI` (the
@solidrt/cli root, so `bun <cli>/bin/srt bundle --json` and `srt check`
run by command name) and `SRT_BUN`; srt sets all three, and a checkout
needs only `SRT_HOME` (`dist/<triple>`, `packages/cli`, bun from PATH).
`srt bundle --json [--server host:port]` is the rebuild contract (one
`BundleOutput` on stdout); the startup typecheck is `srt check <entry>`,
which bundles in memory once more (cheap, not awaited; on a build failure
its compile errors print a second time after the rebuild's, accepted). A
usage error is an uncaught `Error` (flux has no exit): message and exit 1,
with two stack lines of noise, accepted (a thrown string prints worse).

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

1. Close the flux gaps. Done.
2. `server/` on its own: flags instead of the config blob, mode resolved
   in-process (`server/mode.ts`), registry written in-process
   (`server/registry.ts`, sha256 via `crypto.subtle`), binaries from the
   environment (`server/binaries.ts`), `srt bundle --json` and `srt check`
   spawned by command name; `src/entries/` and `shared/config.ts` gone;
   `commands/server.ts` only translates flags and spawns. Done, verified:
   `srt server`/`srt run` in project mode, `flux server.js <file>` started
   directly with `SRT_HOME` only, control-API reload, duplicate-key refusal,
   record removed on exit. The console can spawn a server from here on.
3. Regroup `src/` into `src/<command>/` folders and `src/lib/`, move
   `server/` to `src/server/`; `bin/srt` becomes the router; `scaffold/`,
   `docs/`, `agents/`, `scripts/` dissolve as above;
   website build, `files`, `CLAUDE.md` pointers follow. Done. `srt android`
   is a command of its own (`src/android/`, replacing `srt client
   --android`: a device is a different thing from a local process; the
   scaffold's `android` script follows), and `run` is documented in
   `src/server/docs.md` (run = server --client). The
   router is `src/main.ts` (bin/srt imports it, so it stays typechecked);
   command modules export `main()` and load on demand. The website composes
   `/tools` from `README.md` + `src/*/docs.md` (a `Mount` variant in
   `website/src/build.ts`, rewriting `<name>/docs.md` links to page URLs)
   and reads the usage template from `src/lib/usage.ts`.
4. Prebuilt `dist/server.js` via the Makefile at release. Done:
   `packages/cli/Makefile` (`bun build src/server/main.ts --target=browser
   --format=esm --external 'flux:*'`, verified to keep the `flux:` imports
   external), release.yml calls `make -C packages/cli dist/server.js`.
   `src/lib/server-bundle.ts` stays for the per-launch build in a checkout.

Done looks like: `packages/cli/src` has no folder whose runtime you have to
ask about, `bin/srt` is a router, and `flux src/server/main.ts` started from
any cwd is a complete dev server.
