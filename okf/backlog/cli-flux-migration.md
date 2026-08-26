---
title: Move the srt dev flow into flux and make ports an output
description: Host run/server/client/mcp in one flux process that binds its own port, owns the server registry, and shells out to bun for bundling and typechecking only; a server starts in its project root or on a single file (never by searching up), one server per key, never identified by port.
tags: [cli, flux, dev-server, mcp, bundler, repl, ports, registry]
created: 2026-07-13
---

# Move the srt dev flow into flux and make ports an output

Reshaped 2026-08-25 from the earlier "collapse the repl/dev-server split"
note. The direction is unchanged; the port-less server design is folded in
because the two are the same move: both are about which process owns the
server's identity.

# Today

`srt` is a bun script. `srt run` is bun spawning two children and driving one
of them:

```
bun (srt: args, Bun.build, fs.watch, repl, tsc, shutdown policy, live.json)
  -> flux  (server/main.ts: serve(), /__control__, /__proxy__, tunnel)
       -> bun bundle-cli.ts   (only on an MCP reload)
  -> solidrt-go --dev-server 127.0.0.1:<port>
```

bun drives flux over loopback `POST /__internal__/{reload,stop,watch,stats}`
and polls `/__internal__/clients` for startup and for shutdown-when-empty.

Identity is the port everywhere: `-s N` is `34884 + N`, the registry folder
is `~/.solidrt/servers/<port>/`, the tunnel key lives there, the MCP bridge
and `srt client -s` dial by it.

## What the split costs

- The registry record is written by bun but describes flux's pid and port.
  `srt server` on SIGTERM removes the record and orphans flux: a live server
  becomes invisible while its port stays taken. A crash leaves the inverse.
- `requireFreePort` probe-binds in bun, flux binds later: a TOCTOU, and the
  only reason a port clash needs a friendly message at all.
- Two `buildReload` copies (`src/dev-server.ts`, `server/rebuild.ts`) and two
  rebuild invocation sites (bun in-process for repl/watcher, flux -> bun
  subprocess for MCP) kept in sync by hand.
- Polling: startup (100 x 100 ms), shutdown-when-empty (2 s), and a
  `/__internal__/watch` GET per change event.
- Two numbers for the user (`-s` server, `-c` client tree) that really mean
  "which project, which data tree"; the first is derivable.
- The MCP bridge errors on more than one server for a project, with no way
  for the agent to pick.

# Target

One flux process hosts `run`, `server`, `client` and `mcp`. bun is a
subprocess for exactly the two things only bun can do:

- `bundle-cli.ts` (exists): `Bun.build` with the Solid babel plugin, for the
  app bundle and isolates. Output already carries code, maps, manifest.
- `typecheck-cli.ts` (new, same shape): `tsc` for `check` and the startup
  typecheck.

Everything else is generic and ports to flux modules: `fs`, `dir`, `path`,
`process`, `subprocess`, `serve`, `p2p`.

```
flux (srt: args, serve(), registry, watcher, repl, control API, tunnel)
  -> bun bundle-cli.ts     (every rebuild, one call site)
  -> bun typecheck-cli.ts  (check, startup typecheck)
  -> solidrt-go --dev-server 127.0.0.1:<bound port>
```

`/__internal__/` disappears. Repl keystroke, watcher event and MCP `reload`
call one in-process `rebuildAndBroadcast()`.

## Ports

The server binds its remembered port, else the first free port from 34884
upward (never an OS-assigned one: servers on a machine should read as
34884, 34885, ...; decided 2026-08-25), unless `--port` is given. The bound
port is an output: written to the registry, passed to the spawned client, printed with
the QR. `-s` goes away. `-c` stays for the client data tree.

Stability: the server folder persists the last bound port and tries it first,
falling back to the scan when taken. In practice a project keeps its port across
restarts, so tunnel tickets (UDP port pinned to the dev port) and client
`recents` stay valid, without anyone choosing a number.

## Bind address

Default is loopback only (`host: 127.0.0.1`). `--lan` binds every interface
as today, and is what prints the LAN address QR and what `srt client
--android` needs; without it the server prints no address, since there is
none to reach. The p2p tunnel (`--tunnel`) is independent of this: it is an
iroh endpoint, not the TCP listener, so ticket-paired devices work with the
loopback default. This closes the open question of `/__control__` being
driveable from the LAN: it is only reachable there when the user asked for
the LAN, and `/__internal__/` (the one route that had a loopback check)
no longer exists.

## Two modes, no searching (decided 2026-08-25)

A dev server starts where the project is; it never walks up looking for a
`package.json`. The argument and the cwd decide the mode, and every
ambiguous combination is an error that names the flag that resolves it:

```
cwd has package.json   argument   mode
yes                    none       project (entry = solidrt.entry, default src/index.tsx)
no                     none       error: no package.json here; pass a file
no                     file       file
yes                    file       error, unless --project <file> (project at cwd, entry
                                  overridden) or --file <file> (ignore the project)
```

- **Project mode**: project root = cwd. Assets, the `/assets/` route, the
  watch scope (`src/` + `assets/`), `.srt-data/`, appId and the manifest all
  hang off it. `projectDirFor`'s upward walk and the bridge's `findProjectDir`
  go away.
- **File mode**: sourceDir = the file's directory, no assets, no `/assets/`
  route, manifest = bundle only, appId from the filename (the existing
  fallback), watch = that directory. This is the probe case; today a probe at
  the repo root silently registers the workspace `package.json` as its
  project and walks the repo's `assets/`.

## One server per key

Registry key = the canonical project root (project mode) or the canonical
file path (file mode). Both kinds register. A second `srt run` on the same
key refuses and prints the running server's port, so resolution never has
more than one candidate and there is no `--name`.

```
~/.solidrt/servers/<hash of key>/
  live.json     { pid, port, address, key, mode, projectDir | file, entry, started }
  port          last bound port (tried first, then 0)
  tunnel.key    iroh secret, stable per key
```

Written and removed by the process that owns the pid and the port, so the
orphan and crash cases collapse to one: a record whose pid is dead is stale,
nothing else. The folder name is never parsed back; `live.json` is the record.

Resolution is one function shared by `srt client` (no flags, from cwd) and
`srt mcp`: the project server whose key is cwd; otherwise the file servers
whose file lies under cwd, if exactly one; otherwise an error listing them.
The control response's `x-solidrt-project` header (now carrying the key)
confirms the match, as `mcp.ts` does today. `-s` goes away; `--port` stays
as the explicit override on `run`/`server`/`mcp`.

## Commands under the same table

`srt run` = `srt server` + one attached local client; both take the mode
table above. `srt client` with no flags resolves from cwd exactly like
`srt mcp`; `--server <host[:port]>` stays for remote servers. The local
client is spawned by the server process itself (it alone knows the bound
port) as `solidrt-go --dev-server 127.0.0.1:<port> --data-root
~/.solidrt/clients --client <M>`; the exit policy (last client gone -> server
exits) runs in-process against the ws client set, no polling. `-c` defaults
to 0; storage is per appId under a client tree, so only two clients of the
same app need distinct slots. `srt client --android` requires a server
started with `--lan` (or `--tunnel`) and takes the address from the record.

File-mode build outputs (isolate bundles, the proxy cache) live under the
server folder `~/.solidrt/servers/<key hash>/`, not in a `.srt-data/` next
to the file: nothing owns that directory. Project mode keeps
`<project>/.srt-data/`.

The repl `load <file>` command is dropped: it moved the entry mid-session,
which under one-server-per-key would change the key. Agents use `reload`;
humans restart.

## Packaging

`bin/srt` is a bun shim (`#!/usr/bin/env bun` importing `src/main.ts`) and
stays one: resolving the platform package's binaries is node module
resolution, which is bun's job, and the launcher already does nothing but
resolve, build the config and spawn `flux`. The server bundle is prebuilt
at release time (`scripts/build-server.ts` -> `dist/server.js`, shipped in
the package; a checkout builds it per launch into a temp file when the
prebuilt is absent), so an installed srt never bundles the server.

The shim exec'ing `flux` directly, and `srt pack --flux` packing srt as a
flux app, wait for a flux-side story for finding `solidrt-go`, `bun` and
the platform package; until then the extra move buys nothing.

`bundle`, `pack` and `render` are one-shot bun commands under the same mode
table as `run`: `srt bundle` builds the project at the cwd, `srt bundle
<file>` a file on its own, `--project`/`--file` resolve a file in a project
root. The build root is the cwd (`dist/bundle`, `dist/render`, `dist/pack`
under it). `check` is the one command that walks up from each entry: it
verifies trees of entries from one cwd. Isolate modules are a project
feature: a file on its own bundles none (decided 2026-08-25).

# Flux gaps to close first

Checked 2026-08-25; each is its own small item and useful on its own:

- **Bound port introspection.** DONE 2026-08-25 (uncommitted): `port` is
  optional in `serve()` (default 0) and `Server.port`/`url` report the
  address actually bound.
- **Directory watch.** DONE 2026-08-25 (uncommitted): `dir(path).watch()`
  in `flux:fs` (notify-backed, recursive optional, undebounced, rename
  target reported), and reload-on-save on top of it from the bundle's
  inputs: [done/reload-on-save.md](../done/reload-on-save.md).
- **stdin/tty**: [stdin-tty-support.md](../plans/stdin-tty-support.md). The repl is
  the only consumer; the rest of the migration does not wait on it (the
  no-tty path already runs without a repl).
- **sha256.** DONE 2026-08-25 (uncommitted): `crypto.subtle.digest`
  (SHA-256/384/512): core in `forge/src/crypto.rs`, marshalling in
  `flux/src/standards_plugins/crypto.rs`,
  replacing `Bun.CryptoHasher` for manifest and asset hashing once the CLI
  moves.

# Stages

1. Flux gaps: bound port and `crypto.subtle.digest` (both done); fs watch
   deferred, see above.
2. DONE 2026-08-25 (uncommitted): the dev server (`packages/cli/server/`)
   owns port (remembered, else first free from 34884, or `--port`), record, local client and
   bundle; bun is a launcher (`commands/server.ts`: mode, binaries, config,
   signal relay) plus the registry readers (`client`, `mcp`, `--android`)
   and the bundle-cli subprocess. `-s` removed, `--file`, `--project`,
   `--lan` added, `--port` kept; loopback bind by default; `/__internal__/`,
   repl, watcher, `load` and `watch` gone. `srt mcp` stays bun for good: it
   is a stdio JSON-RPC server on the MCP SDK, and flux has no stdin.
   Verified: file mode on a root probe (OS port, record, reload, MCP
   resolution from the repo root, duplicate refused, SIGTERM drops the
   record), project mode in examples/hello-world (assets route, `srt client`
   from the root, ambiguity error). Found and fixed on the way: `flux:process`
   `on()` unsubscribe never stopped the OS signal watcher, so a server that
   unsubscribed at shutdown never went idle (`flux/tests/process.rs`).
   Also added: `flux:process.pid`, `flux:fs` `file().remove()`.
3. DONE 2026-08-25 (uncommitted): `dist/server.js` prebuilt at release
   (`scripts/build-server.ts`, release.yml step, `files` entry), used when
   present; `typecheck-cli.ts` restores the startup typecheck (spawned by
   the server after the initial bundle, not awaited, prebuilt entries
   skipped); `bundle`/`pack`/`render` under the mode table, build root =
   cwd, `projectDirFor` and the upward walk gone except in `check`; file
   mode bundles no isolates. Scaffold scripts follow (`srt run`, `srt pack
   -o out`). Open: the self-pack, see Packaging. The shim exec'ing flux is
   settled by [done/srt-command-folders.md](../done/srt-command-folders.md):
   `bin/srt` stays a bun launcher that spawns the flux server (one process,
   complete on its own, that the console spawns directly), and the layout
   named under Related there has been regrouped into `src/<command>/`.

# Deliberately not in scope

- mDNS: [mdns-discovery.md](mdns-discovery.md). Port-less servers make the
  ticket/QR story more important, not less.

# Related

- `packages/cli/src/{main,args,dev-server,dev-client,dev-dir,repl,watcher,bundler,bundle-cli}.ts`
- `packages/cli/src/commands/{server,client,mcp}.ts`
- `packages/cli/server/{main,state,control,rebuild,tunnel}.ts`
- `flux/src/forge_plugins/{serve,fs,process,subprocess}.rs`
- [done/parallel-dev-servers.md](../done/parallel-dev-servers.md): the
  port-keyed design this supersedes; its client-tree split (`-c`,
  `~/.solidrt/clients/client<M>/`) stays.
