---
type: backlog-item
title: Several dev servers on one machine, each with its own clients and MCP route
description: Inventory of what already supports running more than one dev server side by side (--port, multi-client servers, numbered client data trees, a stateless MCP bridge that already reports entry/projectDir) and what is missing (colliding client data trees, manual port allocation, and an MCP config that is static per workspace so the agent cannot be pointed at the right server). Scoped 2026-08-08 to one dev server per project folder, which makes the project root the routing key; leading candidate is a project-local .srt-data/server.json marker the bridge finds by walking up from its cwd.
status: open
timestamp: 2026-08-08T00:00:00Z
---

# Several dev servers on one machine, each with its own clients and MCP route

Goal shape: `srt run --port N` should be the whole story. Starting a server
on its own port brings up its own client with its own data folder, further
clients can attach to that server, and a coding agent's MCP tools reach the
server for the project it is working in - without hand-editing config per
port.

This file is an inventory of the pieces, not a plan. A lot of the machinery
is already there; the missing parts are identity (whose data folder, whose
server) rather than transport.

## What already works

- **Port selection.** `--port <N>` is valid on `run`, `server` and `mcp`
  (`packages/cli/src/args.ts`, validateArgs). It is resolved once into
  `DEV_PORT` (`packages/cli/src/dev-server.ts`) and from there reaches every
  consumer: the spawned server's config, the local client's `--dev-server`
  address, the Android client's address, and the MCP bridge's control base.
  A standalone client carries the port in `--server <host:port>` instead.
- **Port clash is already diagnosed.** `requireFreePort()` claims the port
  in srt before spawning the flux server, turning a bare non-zero exit into
  "Port N is already in use; start on another port with --port <N>".
- **Many clients per server.** The server keeps a client map with ids,
  reload broadcasts to all of them, and every control endpoint takes a
  client id (`packages/cli/server/control.ts`). `list_clients` reports
  platform, version, profile, capabilities and answerable query kinds. The
  scaffold AGENTS.md already documents driving several clients at once.
- **Numbered client data trees exist.** `--data-root <dir>` and
  `--client <N>` resolve to `<root>/client<N>/` with `identity/`,
  `apps/<app-id>/{data,cache}` and `logs/` (`lattice/src/storage.rs`),
  forwarded by `clientStorageArgs()` in `packages/cli/src/args.ts`. So the
  mechanism for "its own data folder" is in place; only the choosing of the
  number is not.
- **The MCP bridge is stateless glue.** Every tool call is one HTTP request
  to `http://127.0.0.1:<DEV_PORT>/__control__/...`, so any number of bridges
  and agents can talk to a server, and a bridge survives server restarts
  (`packages/cli/src/commands/mcp.ts`).
- **The server already publishes its identity.** `/__control__/clients`
  returns `generation`, `entry` (the source file it currently serves) and
  `projectDir` (the root it was started in). That is exactly the identity a
  router would key on, and it is already reachable over the control port
  with no new state.

## What is missing

1. **Clients started against different servers share one data tree.**
   Every client defaults to client 0 under the same platform pref root, so
   two `srt run` invocations on different ports both bring up `client0/`:
   shared `identity/` (one persisted iroh key used by two live processes),
   shared `logs/`, and - when both serve the same app id, e.g. two worktrees
   of one project - the same `apps/<app-id>/data` sandbox. Nothing detects
   or prevents this.

   This is a **revisit of two explicit decisions**, not a bug fix.
   `lattice/src/storage.rs` states the client number is "chosen by the user,
   never auto-allocated, so a client's data and identity stay put across
   runs", and `okf/plans/client-storage-updates.md` records that a planned
   running-instance lock was "dropped instead of postponed - multiple
   instances of one client/app are allowed, file consistency is the app's
   responsibility". Any auto-allocation has to argue against those, or the
   choice has to move up into srt (which knows the port) and leave storage
   semantics alone.

   Note the same collision already exists for a second client attached to
   *one* server (`srt client --server host:port` twice): it is not specific
   to multiple servers, it just becomes unavoidable there.

2. **Port allocation is manual.** The clash is reported well, but the user
   still picks the next number by hand and then has to remember it for every
   later `srt client --server` and MCP invocation. Nothing hands out or
   records a port.

3. **No way to enumerate the dev servers running on this machine.** No
   registry file, no lock dir, no listing command. The only discovery route
   today is probing control ports. (`okf/backlog/mdns-discovery.md` covers
   the *network* discovery question and is currently a dead end; the
   same-machine case does not need mDNS at all, which is worth keeping
   separate.)

4. **MCP config is static per workspace, so the agent cannot pick.** The
   scaffold ships one entry (`packages/cli/scaffold/mcp.json`:
   `bun node_modules/@solidrt/cli/bin/srt mcp`) with no port, so the bridge
   always dials 34884. The current documented answer is to duplicate the
   flag into the config by hand - scaffold AGENTS.md: "if the user started
   it with `--port N`, .mcp.json needs the same flag". That does not scale
   past one server and goes stale the moment the port changes. The agent
   chooses a *tool*, never a port, so nothing at call time can redirect it.

## Decided: one project folder, one dev server (2026-08-08)

The driving case is several *projects* side by side (or worktrees of one),
each in its own editor window with its own server, client and agent. One
folder running two servers is explicitly **out of scope**. That ruling is
what makes the rest simple: the project root becomes a unique key for "which
server", and both sides can compute it independently without agreeing on
anything in advance.

It also means no router, no `server` argument on the tools, and no listing
of running servers is needed for this item. Enumerating servers may still be
worth having for its own sake, but it is not on the critical path here.

## Leading candidate: a project-local marker file

Routing by marker, in both directions from the same key:

- `srt run` / `srt server` writes `.srt-data/server.json` into the project
  root it was started in: `{ port, pid, entry }`.
- `srt mcp` walks up from its own cwd to the project root (the same
  tsconfig/package.json walk `findProjectRoot` in
  `packages/cli/src/commands/check.ts` already does), reads the marker, and
  dials that port.

`.srt-data/` is already the project-local server state dir (the proxy HTTP
cache lives there) and is already gitignored both in this repo and in
`packages/cli/scaffold/gitignore`, so this adds no new location, no new
ignore rule, and nothing the user sees.

**What the two-window case then looks like.** Window A has project A open
with `srt run` on the default port; window B opens project B with
`srt run --port 34885`. Both projects carry the identical scaffolded
`mcp.json` with no `--port` in it. Bridge A starts with cwd inside project A,
finds A's marker, dials 34884; bridge B finds B's marker, dials 34885. The
two windows are configured identically and never collide, because the port
lives next to the project that owns it rather than in the agent config.

**Why this is generic and not VS Code shaped.** The only assumption is that
the agent process runs somewhere inside the project directory. That holds
for a terminal agent, an IDE-spawned stdio server, and anything else with a
notion of "the folder I am working in". No harness needs variable
substitution, per-window env, or dynamic config.

**Resolution order** (back-compatible at both ends): explicit `--port` wins,
exactly as today; else the marker found by the walk-up; else the default
34884, so an existing single-server setup behaves identically.

### Wrinkles to handle

- The cwd assumption is the one genuinely environment-dependent part. A
  harness that spawns the bridge from the home directory finds no marker and
  falls back to the default port - acceptable, provided the message says
  what was looked for and where, and `--port` stays as the escape hatch.
- Stale markers after a crash. Do not trust the file: probe the port, and if
  the control API does not answer or answers with a different `projectDir`,
  treat the marker as dead.
- `cacheDir: resolve(".srt-data")` in `packages/cli/src/dev-server.ts`
  resolves against srt's cwd, not `state.projectDir`. The marker must be
  written to the project root specifically, or `srt run` from a subfolder
  puts it where the walk-up will not look.

**Rejected: scanning a port band.** Probing ports from the default upward
and matching `projectDir` needs no new on-disk state, but it breaks the
moment someone picks 9000, and it still has to match `projectDir` to
disambiguate. The marker does the same job with less machinery and no bound
on port choice. A per-user registry dir has the same profile plus stale-entry
cleanup, and is only worth it if servers ever need enumerating for other
reasons.

## Still open on the data-folder half

- **Derive the client number where the port is known.** srt, not storage,
  defaults `--client` from the port (default port keeps client 0, so
  existing trees and behavior are untouched). Keeps storage's "never
  auto-allocated" contract intact, since the number still arrives as an
  explicit flag. Alternative: derive it from the project root instead, which
  survives a port change - but named client dirs were explicitly retired in
  the storage plan, so that is a third decision to reopen, not a free option.

## Open questions

- Should `srt run` auto-pick a free port when the default is taken? It makes
  "start another one" a single command, but only if the port is then
  announced clearly and the MCP side can find it anyway.
- How do extra clients attached to the *same* server get distinct trees:
  documented explicit `--client N`, or claim-the-first-free (which needs the
  dropped lock back)?
- Does anything else assume it is the only dev session on the machine
  (Android install/launch over adb, the p2p tunnel ticket and its key dir,
  capture files) in a way that breaks with two servers running side by side?
  The project-local ones are covered by the one-folder-one-server ruling;
  the machine-global ones are not.

Related: `okf/plans/client-storage-updates.md` (the storage decisions this
would revisit), `okf/plans/dev-server-launch-targets.md` (launching clients
from a running session - the same "which server, which client" identity
problem from the other end), `okf/backlog/mdns-discovery.md` (network
discovery, deliberately separate), `okf/backlog/mcp-agent-loop-improvements.md`,
`docs/flux-dev-server-plan.md`.
