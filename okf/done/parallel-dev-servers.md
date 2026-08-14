---
title: Several dev servers on one machine, each with its own clients and MCP route
description: Design decided 2026-08-13. Two numbers - `--session`/`-s N` picks the dev-server port (34884 + N) and `--client`/`-c M` picks the client data tree (defaulting to the session number) - plus a three-folder split by ownership - server state keyed by port in `~/.solidrt/servers/<port>/` (dev tooling, a single home dotdir on every platform), project state in the project's .srt-data/, client state in ~/.solidrt/clients/client<M>/ - reached through the --data-root flag that already exists, so the client runtime needs no change and nothing dev-related is left under the SDL pref path. A server run serves the project it started in (`load` outside the project root is refused) while the server identity itself, keyed by port, is independent of any project and outlives every one it serves. The MCP bridge resolves its port per call from the global server registry by matching projectDir, so the scaffold's mcp.json never carries a port. Supersedes the 2026-08-08 scoping to one server per project folder and its project-local marker file.
created: 2026-08-08
completed: 2026-08-13
---

# Several dev servers on one machine, each with its own clients and MCP route

Goal shape: `srt run -s1` is the whole story. It starts a dev server on its
own port with its own client and its own data folder, `srt client -s1 -c2`
attaches a second client to it, and a coding agent's MCP tools reach the
server for the project it is working in without hand-editing config per
port.

The machinery for transport was already there (see the inventory below);
what was missing was identity - whose data folder, whose server. The
2026-08-13 design answers that with two explicit numbers and a rule for
where each kind of state lives.

## Decided 2026-08-13

### The two numbers

- `--session <N>` / `-s <N>` (also `-s1`, `--session=1`), default 0. Selects
  the dev server: **port = 34884 + N**. Session 0 is today's default port,
  so existing single-server use is unchanged. Valid on `run`, `server`,
  `client` and `mcp`.
- `--client <M>` / `-c <M>`, **defaulting to the session number**. Selects
  the client data tree, exactly as the existing `--client` flag does. The
  default keeps the one-client-per-server case at a single flag, and slots
  stay stable across restarts.
- `--port <P>` still wins where it is already valid, and does not conflict
  with `-s`: the session then only supplies the client slot, and the server
  folder is keyed by the port actually bound.
- Validation: non-negative integer, and `34884 + N` must stay a legal port.

`-c` is currently `--compile`. Node's `parseArgs` has one global option
table, so shorts cannot be scoped per command; `--compile` (a `bundle`-only
flag, typed rarely and usually from a script) gives up its short, and `-c`
goes to `--client`, which is typed interactively next to `-s`. All four
spellings (`-s1`, `-s 1`, `--session 1`, `--session=1`) parse correctly with
`parseArgs` under bun - verified 2026-08-13.

### Three folders, one rule: whose property is it?

```
~/.solidrt/servers/<port>/                  machine + port (dev tooling)
  tunnel.key      iroh secret for the p2p tunnel
  live.json       { pid, port, projectDir, entry, started }

<project>/.srt-data/                    the source tree
  http-cache.db                   proxy cache (--proxy-http only)
  typecheck-<pid>.tsconfig.json   transient

~/.solidrt/clients/client<M>/               the client instance (dev)
  identity/p2p.key  config.json  logs/  apps/<app-id>/{data,cache,versions}
```

**The server folder is keyed by port, not by session ordinal.** `-s1` and
`--port 34885` are two spellings of the same server, and the tunnel ticket
pins the UDP port to the dev port, so the identity and the port must not be
able to drift apart. Keying by port also makes a restart on the same port
reuse the same key, which is what lets a paired client re-dial an old
ticket after the server is restarted from a different project folder.

**All dev state lives in one home dotdir** (decided 2026-08-13). srt is a
CLI tool on a dev machine, so `~/.solidrt/` (`%USERPROFILE%\.srt\` on Windows)
holds both halves, resolved with `os.homedir()` and one rule on every
platform. `rm -rf ~/.solidrt` resets every bit of dev state at once.

**Dev clients get there through the `--data-root` flag that already
exists** - no new mechanism and no Rust change. `--data-root <dir>` already
resolves to `<dir>/client<N>/{identity,config.json,logs,apps/<id>/{data,cache}}`,
which is the identical shape to today's dev tree minus the vendor levels,
and `clientStorageArgs()` already forwards it. srt simply always passes
`--data-root ~/.solidrt/clients` for the clients it spawns locally. The client
keeps exactly one default rule (`SDL_GetPrefPath`) and gains no
platform-conditional branch; srt just does not hand a host path to an
Android device, which is a choice about what to forward, not a special case
in the runtime.

The seam this creates, worth knowing about: a go client launched **directly**
(bare `solidrt-go`, no srt) still resolves pref path, so it sees a
different, empty store than the same binary launched through `srt client`.
On desktop the client is essentially always started through srt, and on
Android and the TV it is started by the OS and never gets a data root, so
this is invisible in practice - but it is real, and someone will hit it
while debugging.

**What is left under the pref path** is then only the shipped launcher and
installed packed apps - no dev trees at all. Where exactly those land, and
why an end user's filesystem should not advertise the engine that built
their app, is `okf/backlog/app-storage-vendor-path.md`. That item removes
the `SolidRT/go/` split entirely, so this one no longer needs the vendor-dir
rename it briefly carried, and stage 3 below does not depend on it.

**The launcher has exactly one client**, since one install is one client, so
the `client<N>` level does not belong in its tree at all. With dev clients
moved to `~/.solidrt/clients/`, that collapses the layouts in
`lattice/src/storage.rs` rather than adding to them:

| layout | when | shape |
|---|---|---|
| dev | `--data-root` (srt always passes it) | `<root>/client<N>/` + `apps/<id>/` - many clients, many apps |
| packed app | app id from the pack manifest | flat - one client, one app |
| launcher | neither | one client, **many** apps - no client level |

`flat` stops being a boolean meaning "one client and one app" and becomes
two independent facts: does a client level exist, does an apps level exist.
Those three rows are the only combinations; the fourth (one app, many
clients) is meaningless.

That also makes **`--client` a data-root-only flag**, which is a rule that
already exists in this exact shape - storage.rs already warns and ignores it
for a packed app ("--client does not apply to a packed app, ignoring") and
now does the same whenever there is no explicit root. Since srt always
forwards `--data-root` for dev clients, `-c` keeps working everywhere it is
meant to.

**Project state is resolved against `state.projectDir`, not srt's cwd.**
Today `cacheDir: resolve(".srt-data")` and `keyDir: process.cwd()` in
`packages/cli/src/dev-server.ts` resolve against the invoking directory, so
`srt run` from a subfolder scatters them.

**The tunnel key moves** from `<project>/.srt-tunnel-key` to
`servers/<port>/tunnel.key`. Existing project-root key files go stale: the
ticket changes once on the first run after the move, then stays stable.
Old `.srt-data/http-cache.db` files are caches and can be deleted.

### A server run serves the project it started in

A server is not owned by a project. Its identity is `(machine, port)` and
outlives every project it ever serves: that is why the key lives in
`servers/<port>/tunnel.key` and not in the project, and why restarting the
same session from a different folder keeps the same ticket. What is fixed
is the **run**: `state.projectDir` is set at startup and does not move for
the life of the process, and the project association is per-run state that
appears in `live.json` at bind and disappears at exit. Exit a server and it
is free to pick up any other project on the next start.

Within a run, the repl's `load` gets the same
project-root check `/__control__/load` already applies
(`packages/cli/server/control.ts`), so there is one rule instead of two. To
work on another project, restart the server there - on the same port if you
want the same tunnel identity, or on another session to run both at once.
Clients drop and reconnect on their own.

This is not a lost feature but a removed inconsistency. A cross-project
`load` today retargets only half the system:

- **Follows the load**: `sourceDir` and `projectDir` move
  (`packages/cli/src/repl.ts`), so the server's `/assets/` root moves with
  them and the first project's assets stop being served; the manifest is
  rebuilt from the new project, so its icon, fonts, displayName and
  **appId** are the new project's; the client re-anchors its cwd into the
  new app's sandbox.
- **Does not follow**: the HTTP cache and any project-local marker stay
  with the launch project, and the file watcher already stays on the
  launch-time source - `control.ts` documents that as a known wart ("a
  watcher started on the launch-time source keeps watching that file").

On the client, whether the second app overwrites the first depends only on
the app id, never on the folder (`appId` from `solidrt.appId`, else the
package name, else the entry filename - `packages/cli/src/project.ts`).
Different ids install side by side as two apps, each with its own `data/`
and `cache/`; the same id installs over the first version and inherits its
data sandbox, silently. Restarting the server does all of this correctly
and for free, so the alternative - making everything follow a load (reopen
the cache at the new path, move the watcher, decide what an app-id change
means for the client store) - was not worth building.

### MCP resolution, per tool call

The harness starts the bridge when the workspace opens, usually before any
dev server exists, and keeps it alive across server restarts. So resolution
cannot happen once at bridge startup; it happens **per tool call**, which is
cheap (a directory listing plus a few small JSON reads) and means the agent
follows a server restarted onto a different session without being
restarted itself.

1. Explicit `-s N` (or `--port N`) wins.
2. Otherwise: walk up from the bridge's own cwd until a `package.json` is
   found. That yields a path and nothing is read there. Then list
   `~/.solidrt/servers/*/live.json` - the global registry, the
   only place server records exist - and keep records whose `projectDir`
   equals that path and whose `pid` is alive.
   - exactly one: dial it.
   - several: error listing them, e.g. "2 dev servers are serving this
     project (ports 34884, 34885); pass -s N".
   - none: error naming what was searched for, e.g. "No dev server for
     /path/to/project. Start one with srt run, or pass -s N."
3. Probe `/__control__/clients` on the resolved port before use - it already
   returns `projectDir` - and reject on disagreement. `live.json` is a hint;
   the probe is authoritative, which is also how a stale record left by a
   `kill -9` is caught.

The project folder holds no server state and does not participate: the link
is a `projectDir` string stored on the server side, compared against a path
the bridge derives locally.

Net effect: the scaffold's `mcp.json` never changes and never carries a
port, which is what this item set out to fix. Two editor windows on two
projects ship identical config and never collide.

The one environment-dependent assumption is that the bridge process runs
somewhere inside the project. That holds for VS Code, Claude Code and a
terminal agent; when it does not, the error says which directory it
searched from and `-s N` is the escape hatch.

## Stages

Stages 1 and 2 implemented 2026-08-13 and verified live: a session-1 server
bound 34885 with tunnel.key and live.json in `~/.solidrt/servers/34885/` and the
proxy cache in the project's `.srt-data/`; the MCP bridge resolved the server
from the project dir (and refused from a different project), followed the
explicit `-s` escape hatch, and the registry record disappeared on SIGTERM.
Two deltas from the text below, both consistency-tightening: the repl `load`
no longer retargets `projectDir` at all (the project root and /assets/ root
are fixed for the life of the run, exactly like `/__control__/load`), and a
`server`/`run` started without an entry derives its projectDir by walking up
from cwd to the nearest package.json instead of taking cwd itself, so the
registry record always matches what the bridge derives.

Stage 3 implemented 2026-08-13, all lattice tests pass. Two deltas from the
text below: the marker (`client<M>/run.pid`) is liveness-checked by holding
an OS file lock on it for the life of the process (`File::try_lock`, released
by the OS on any exit including kill -9, so a marker is never stale) rather
than by pid probing - the pid inside only names the holder in the warning.
And it is claimed for `--data-root` trees only, so the packed runtime, the
launcher, and Android never write anything dev-shaped. The internal name for
`~/.solidrt/` is `devDir()` (`packages/cli/src/dev-dir.ts`), with the folder
name isolated in one constant for a later rename or config.

**Stage 1 - the two numbers and the three folders.** CLI only, no Rust.
`-s`/`--session` and `-c`/`--client`; port from the session; client slot
defaulting to the session; the tunnel key moved to
`~/.solidrt/servers/<port>/`; `--data-root ~/.solidrt/clients` forwarded for every
locally spawned client; `.srt-data` resolved against the project root; the
repl's `load` bound to the project root; help text and docs updated.

Files: `packages/cli/src/args.ts` (options, `validateArgs`, `printUsage`,
`clientStorageArgs`), `packages/cli/src/dev-server.ts` (`resolveDevPort`,
`cacheDir`/`keyDir`), `packages/cli/server/tunnel.ts` (key file name),
`packages/cli/src/repl.ts` (load restriction), `packages/cli/src/commands/`
(`client.ts`, `mcp.ts`), `packages/cli/src/dev-client.ts`,
`packages/cli/src/dev-android.ts`, plus a small new module resolving
`~/.solidrt/`. Docs: `docs/cli.md`, `packages/cli/AGENTS.md`, scaffold `AGENTS.md`.

**Stage 2 - the registry and MCP routing.** CLI only. Write and remove
`servers/<port>/live.json` around the server's lifetime; implement the
per-call resolution above in `packages/cli/src/commands/mcp.ts`. An `srt
servers` listing command falls out of the registry for free if wanted.

**Stage 3 - the Rust half.** Independent of the other two and optional.
Stages 1 and 2 work without it, because srt always passes `--data-root` and
that branch of `storage.rs` is already correct.

- Drop the `client<N>` level from the launcher layout and make `--client`
  data-root-only (`lattice/src/storage.rs` plus its tests). See the table
  above.
- A pid marker in `client<M>/` so two live clients sharing one tree (one
  `identity/p2p.key`, one `apps/<id>/data`) are warned about loudly instead
  of silently corrupting each other. A warning, not the running-instance
  lock that `okf/plans/client-storage-updates.md` deliberately dropped.

The vendor-path question (`<pref>/SolidRT/...` for the launcher and packed
apps) is **not** part of this item - see
`okf/backlog/app-storage-vendor-path.md`.

## What already works

- **Port selection.** `--port <N>` is valid on `run`, `server` and `mcp`
  (`packages/cli/src/args.ts`, validateArgs), resolved once into `DEV_PORT`
  (`packages/cli/src/dev-server.ts`) and from there reaching the spawned
  server's config, the local client's `--dev-server` address, the Android
  client's address, and the MCP bridge's control base. A standalone client
  carries the port in `--server <host:port>` instead.
- **Port clash is already diagnosed.** `requireFreePort()` claims the port
  in srt before spawning the flux server, turning a bare non-zero exit into
  "Port N is already in use; start on another port with --port <N>".
- **Many clients per server.** The server keeps a client map with ids,
  reload broadcasts to all of them, and every control endpoint takes a
  client id (`packages/cli/server/control.ts`). `list_clients` reports
  platform, version, profile, capabilities and answerable query kinds.
- **Numbered client data trees exist.** `--data-root <dir>` and
  `--client <N>` resolve to `<root>/client<N>/` with `identity/`,
  `apps/<app-id>/{data,cache}` and `logs/` (`lattice/src/storage.rs`),
  forwarded by `clientStorageArgs()`. The mechanism for "its own data
  folder" is in place; only the choosing of the number was not, and stage 1
  chooses it in srt, so storage's "never auto-allocated" contract is
  untouched - the number still arrives as an explicit flag.
- **The MCP bridge is stateless glue.** Every tool call is one HTTP request
  to `http://127.0.0.1:<DEV_PORT>/__control__/...`, so any number of
  bridges can talk to a server and a bridge survives server restarts
  (`packages/cli/src/commands/mcp.ts`).
- **The server already publishes its identity.** `/__control__/clients`
  returns `generation`, `entry` and `projectDir` - exactly what the
  resolution probe needs, already reachable with no new state.

## Rejected

- **Session ordinal as the server folder key.** Breaks the moment `--port`
  is used: the same server would get two identities, and the tunnel ticket
  pins the port.
- **A project-local server marker (`.srt-data/server.json`).** The
  2026-08-08 leading candidate. Redundant with the registry once server
  state lives in one place, and it puts a stale file inside the project
  after a crash instead of in one central place where pid liveness handles
  it. Two servers on one project would also have had to fight over it.
- **A retargetable server.** See above: it half-retargets today, and
  restarting is one command now that sessions exist.
- **A session level in the client tree** (`go/s<N>/client<M>/`). A client's
  identity is a property of the client, not of whichever server it happens
  to dial; the tree is already keyed by app id inside, so switching projects
  is handled. It would also cost a Rust change and orphan existing trees.
- **Auto-allocated client slots** (claim the first free). Deferred, and the
  reason for deferring got weaker: the original objection was that it must
  live in the client, since only the client knew the pref path. With dev
  trees in `~/.solidrt/clients/`, srt knows the path too and could scan for a
  free slot before spawning, leaving storage's "never auto-allocated"
  contract intact because the number still arrives as an explicit flag. It
  still needs a liveness signal to know which slots are taken, which is
  stage 3's pid marker. Worth revisiting once that exists and if the
  explicit number chafes.
- **Scanning a port band** (2026-08-08). Breaks when someone picks 9000 and
  still has to match `projectDir` to disambiguate.
- **XDG dirs for the server state**, i.e. `~/.local/share/srt/servers/` for
  the key plus `$XDG_RUNTIME_DIR/srt/` for the registry. More correct on
  Linux, and `XDG_RUNTIME_DIR` would have cleared stale registry records at
  logout for free. Deferred, not dismissed: it needs a second and third rule
  for Windows and macOS (`%LOCALAPPDATA%` + `%TEMP%`,
  `~/Library/Application Support` + `$TMPDIR`), where `~/.solidrt/` is one rule
  everywhere and matches what `~/.cargo` and `~/.bun` do. Decision
  2026-08-13 was to try the dotdir and see how it wears. Now tracked on its
  own as `okf/backlog/xdg-storage-layout.md`, which carries the purpose
  mapping, the options, and the reason it is not just a rename: a client
  tree is one directory whose subdirs have four different XDG purposes.

## Open

- **Same app id across different projects.** Two projects that both default
  their appId to e.g. `app` (or two worktrees of one project) share
  `apps/<id>/data` and overwrite each other's installed version, across
  sessions and across time. Not created by this item and not fixed by it.
  Cheap catch if wanted: record `projectDir` in the client's `state.json`
  and warn when an install replaces a version that came from a different
  one.
- **`http-cache.db` under two servers.** Binding does not stop two sessions
  from serving the same project folder, which puts two flux processes on one
  sqlite file. WAL is the obvious answer; it is unverified whether
  `flux:sqlite` passes `PRAGMA journal_mode=WAL` through.
- **Stale registry records.** `~/.solidrt/` persists across reboots, so
  `live.json` cleanup rests entirely on the pid check plus the control
  probe. Nothing sweeps records whose pid was reused by an unrelated
  process; the probe catches that (wrong or absent `projectDir`), but a
  sweep of records older than the boot time would be tidier if it ever
  bites.
- **Auto-picking a free port.** Still manual. Sessions make "start another
  one" cheap enough that this may never be worth it.
- **Anything else assuming a single dev session per machine**: Android
  install/launch over adb, capture files. The project-local ones are covered
  by binding; the machine-global ones are not audited.

Related: `okf/plans/client-storage-updates.md` (the storage decisions this
builds on), `okf/plans/dev-server-launch-targets.md` (launching clients
from a running session), `okf/backlog/mdns-discovery.md` (network
discovery, deliberately separate), `okf/backlog/mcp-agent-loop-improvements.md`,
`docs/flux-dev-server-plan.md`.
