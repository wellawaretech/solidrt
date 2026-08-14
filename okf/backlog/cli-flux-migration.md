---
title: Move the srt CLI fully into flux
description: Collapse the repl/dev-server split into one flux process so there is exactly one rebuild-and-push path, leaving Bun only as a bundler subprocess.
tags: [cli, flux, dev-server, mcp, bundler, repl]
created: 2026-07-13
---

# Move the srt CLI fully into flux

The MCP `reload` need was addressed without the full migration, by making the
**dev server** (already a flux process) the rebuild authority for the on-demand
path, and accepting a Bun subprocess as the (inherent, not optional) bundler:
flux cannot call `Bun.build`, so a flux-hosted rebuild *must* shell out.

What shipped:

- `bundler.ts` split into a pure `bundleWith({ entry, devBase, dev, minify })`
  (no ambient `args`/`state`/`print`) plus the old ambient `bundle()` wrapper.
- `packages/cli/src/bundle-cli.ts`: standalone Bun entry, params as one JSON
  argv, bundled code to stdout. The single external bundler invocation.
- `server/rebuild.ts` `rebuildAndBroadcast()`: spawns `bundlerCmd` via
  `flux:subprocess`, latches + broadcasts the reload. `Config` gained `entry`,
  `minify`, `bundlerCmd` ([bunPath, bundleCliPath], set at spawn in
  `dev-server.ts`). `load` piggybacks `entry` on its `/__internal__/reload` POST
  so a later MCP reload bundles the loaded file.
- `POST /__control__/reload` -> `rebuildAndBroadcast()`; new `reload` MCP tool.

Correction to earlier scope notes: `proxyHttp` is a reload-*message* flag
(server already holds it in `state.config`; `proxyFiles` was sunset
2026-07-21), not a build input, so the bundler subprocess only needs
entry/devBase/dev/minify.

**Still open (the reason this stays deferred):** the repl's keystroke reload and
watcher still bundle in-process (same `bundle()` function, so no logic drift,
but a second *invocation* site). Folding the repl onto `rebuildAndBroadcast()`,
and ultimately the whole CLI into flux, remains the "clean this up" direction
below.

# Problem

Two related asks surfaced together:

1. Claude Code's `Edit` tool writes files atomically (temp file + rename onto
   the real path). `packages/cli/src/watcher.ts`'s `fs.watch` only ever sees
   the rename event with the *temp file's name*, which fails the
   `.tsx?/.jsx?` extension filter, so edits made by an agent are silently
   never picked up (confirmed by direct reproduction: a Bun `fs.watch`
   listener never logs an event for the real destination name, only the temp
   name).
2. Even if that filter is fixed, agents make many edits in a burst - we do
   not want a reload firing per edit. The actual want is an **explicit**
   reload, triggered on demand (e.g. via an MCP tool), not automatic
   file-watching for agent-driven changes.

# Why this got architecturally deep

An MCP `reload` tool is not a simple addition. `srt mcp`
(`packages/cli/src/commands/mcp.ts`) is deliberately "stateless glue" - a
separate OS process from the interactive `srt` REPL that only does read-only
HTTP GETs against `/__control__/`. It has no access to the live REPL's
`bundle()` config (`state.serverUrl`/devBase, `values.dev`,
`values["proxy-http"]`, `values.minify` - all process-local). And there is no reverse channel: the dev server's
`/__internal__/` IPC is one-directional (REPL -> server only).

Options considered, in order, and why each was rejected:

- **Loosen the watcher's rename-event filter.** Fixes symptom 1 in isolation
  (verified working) but does not address "many edits, one reload" at all -
  wrong tool for the actual ask. Reverted.
- **MCP bundles independently**, given a `file` argument, by duplicating the
  live session's config. Rejected: two independently-maintained copies of
  "how to bundle" that can drift.
- **MCP routes to the server, server spawns a bundler subprocess; REPL keeps
  bundling in-process as it does today.** Avoids a redundant subprocess spawn
  for the common REPL-driven case, but leaves **two different code paths**
  that can execute a rebuild (REPL in-process vs. MCP-triggered subprocess) -
  exactly the kind of split that causes silent divergence later.
- **Server becomes sole authority for "rebuild and push"; REPL also routes
  through it**, always via subprocess, even though the REPL's own process
  technically could bundle directly. One path, small constant overhead (one
  extra subprocess spawn per REPL-triggered reload, negligible for a dev
  tool). This is directionally right but still leaves Bun as the general CLI
  runtime with flux as a secondary process it spawns and drives.

# Chosen direction

Move the entire CLI - REPL command handling, dev-server logic, the surface
MCP talks to - into a single flux-hosted process. Bun's role shrinks to
exactly one thing: an external bundler, invoked via `flux:subprocess`, from
one place, used identically regardless of trigger (REPL keystroke, file
watcher, MCP call). One process means there is structurally only one path,
not an invariant we have to maintain by discipline.

# Scope, as actually investigated (not guessed)

**Ports mechanically:** ~17 files under `packages/cli/src/` touch
`Bun.*`/`node:*` APIs (`args.ts`, `bundler.ts`, `dev-server.ts`, `repl.ts`,
`watcher.ts`, `util.ts`, various `commands/*.ts`, etc.). flux already has
`file`, `fs`, `dir`, `path`, `process`, `subprocess`, `net` modules
(`flux/src/plugins/modules/`) covering most of what these files actually do.
Mostly grunt work, low technical risk.

**Stays external, exactly once:** bundling. `Bun.build` has no QuickJS
equivalent and reimplementing a bundler is out of scope. `bundler.ts`'s
`bundle()` needs to stop reading ambient `state`/`values` singletons and take
explicit params (entry file, devBase, proxyHttp, minify) so it
can run as a standalone script spawned via `flux:subprocess`.

**Side effect worth noting:** `dev-server.ts` currently `Bun.build`s the
dev-server script itself before handing it to `flux` to run
(`bundleServer()` in `dev-server.ts`) - a runtime bootstrap step. Once
the CLI *is* the flux process, there's no separate Bun process left to do
that from. The CLI's own script would need to be pre-built at release time
instead (same treatment `flux`/`fluxc` binaries already get - see the
"prebuilt binary, not rebuilt per run" gotcha in project memory), not
bundled fresh on every launch.

**Depends on (split out separately):**
[stdin/tty support in flux](stdin-tty-support.md). Porting `repl.ts`'s
`node:readline`-based prompt off Bun needs raw-mode terminal input, which
does not exist anywhere in flux/alloy today - genuinely new capability work,
not a port. Scoped as its own backlog item because the capability is useful
independent of this migration (any interactive terminal UI under flux would
need it), and should not be blocked on or scoped by this project committing
to happen.

# Open questions for whoever picks this up

- Exact subprocess entrypoint contract for the extracted bundler script
  (argv shape vs. JSON-on-stdin, how bundled output comes back - stdout vs.
  temp file).
- Release-time build story for the CLI's own now-flux-hosted script, once
  the runtime `Bun.build` bootstrap goes away.
- [stdin/tty support](stdin-tty-support.md) landing is a prerequisite for
  porting `repl.ts` specifically - the rest of this migration (dev-server
  logic, MCP command surface) does not depend on it and could proceed
  independently.

# Related files

- `packages/cli/src/repl.ts`, `watcher.ts`, `dev-server.ts`, `bundler.ts`,
  `commands/mcp.ts`
- `packages/cli/server/main.ts`, `control.ts`, `state.ts`
- `flux/src/plugins/modules/process.rs`, `subprocess.rs`
- `docs/flux-dev-server-plan.md` (prior art on the srt/server split)