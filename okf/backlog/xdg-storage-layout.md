---
title: Dev-tooling storage in XDG base directories instead of one home dotdir
description: Dev state lives in ~/.solidrt/ (servers/<port>/, clients/client<M>/), chosen 2026-08-13 for one rule on every platform. XDG splits by purpose instead - identity in XDG_DATA_HOME, logs and recents in XDG_STATE_HOME, caches in XDG_CACHE_HOME, and live registry records in XDG_RUNTIME_DIR, which would clear stale ones at logout for free. The tension is that a client tree is currently one contiguous directory whose subdirs have four different purposes, so a faithful split scatters it across four roots and --data-root stops being able to express it. Records the purpose mapping, three options from purist to XDG-lite, the Windows/macOS fallback question, and what the split would cost the uninstall path.
created: 2026-08-13
---

# Dev-tooling storage in XDG base directories instead of one home dotdir

All dev state currently lives in one home dotdir, resolved in
`packages/cli/src/dev-dir.ts`:

```
~/.solidrt/servers/<port>/    tunnel.key, live.json
~/.solidrt/clients/client<M>/ identity/, config.json, logs/, apps/<id>/{data,cache,versions}
```

That was decided on 2026-08-13 (see `parallel-dev-servers.md`) for one
reason: one rule on every platform, matching what `~/.cargo` and `~/.bun`
do, against XDG needing a second and third rule for Windows and macOS. The
intent was always to see how it wears. This item is the other direction,
written down while the reasoning is fresh.

The folder name is deliberately isolated in `DEV_DIR_NAME` with every path
going through `devDir()`, so the *name* is a one-line change. The layout is
not, which is what this item is actually about.

## What XDG says

| root | default | for |
|---|---|---|
| `XDG_DATA_HOME` | `~/.local/share` | persistent data the app owns |
| `XDG_CONFIG_HOME` | `~/.config` | user-editable configuration |
| `XDG_STATE_HOME` | `~/.local/state` | persistent but non-portable: logs, history, recents |
| `XDG_CACHE_HOME` | `~/.cache` | regenerable, safe to delete |
| `XDG_RUNTIME_DIR` | `/run/user/<uid>` | sockets, pids, locks. Mode 0700, cleared at logout |

Mapped onto what we store:

| what | purpose | XDG root |
|---|---|---|
| `servers/<port>/tunnel.key` | secret identity, not regenerable | DATA |
| `servers/<port>/live.json` | pid + port + projectDir of a running server | **RUNTIME** |
| `clients/client<M>/identity/p2p.key` | secret identity | DATA |
| `clients/client<M>/apps/<id>/data` | the app's sandbox | DATA |
| `clients/client<M>/apps/<id>/cache` | fetch disk cache | CACHE |
| `clients/client<M>/apps/<id>/versions` | installed bundles, re-pushable | DATA (or CACHE) |
| `clients/client<M>/config.json` | recent dev-server addresses | STATE |
| `clients/client<M>/logs/` | logs | STATE |
| `<project>/.srt-data/` | project-scoped | unchanged, stays in the project |

## The tension

A client tree is currently **one contiguous directory** whose subdirectories
have four different purposes. A faithful XDG split scatters a single client
across four roots, and several things depend on it being one place:

- `--data-root <dir>` is one directory by definition. It could no longer
  express a split tree, so it would need siblings (`--cache-root`, ...) or a
  different shape entirely. srt passing one path per client is what makes
  the current dev layout free of Rust changes.
- Uninstall currently removes `apps/<id>/` and takes the app's data, cache
  and versions with it (`lattice/src/go/store.rs`). Splitting cache out
  means uninstall must clean two roots, and a missed one leaks silently.
- "Delete this client and everything it holds" stops being one `rm -rf`.

The one thing the split genuinely buys, beyond correctness for its own sake,
is `XDG_RUNTIME_DIR` for `live.json`: stale registry records after a
`kill -9` would clear at logout instead of relying on a pid check plus a
control-port probe (which is the open lead in `parallel-dev-servers.md`).

## Options

1. **Faithful split.** Every row above in its proper root. Most correct on
   Linux, most disruptive: it reshapes `--data-root`, the uninstall path,
   and the client's own resolution.
2. **Hybrid.** Identity, data and versions in `XDG_DATA_HOME/solidrt/`,
   runtime records in `XDG_RUNTIME_DIR/solidrt/`, everything else along for
   the ride. Buys the stale-record cleanup and the main correctness point
   without splitting a client's data from its cache. Leading candidate.
3. **XDG-lite.** Relocate the whole dotdir to `XDG_DATA_HOME/solidrt/` and
   keep the internal shape exactly as it is. Nearly free, respects a user's
   `XDG_DATA_HOME` override, ignores the purpose split. Honest about being
   a naming fix rather than a layout fix.

## Open

- **Which option**, and whether the RUNTIME_DIR cleanup is worth the split
  it drags in on its own.
- **Windows and macOS.** They have no XDG, so any of these needs a fallback
  (`%LOCALAPPDATA%` + `%TEMP%`, `~/Library/Application Support` + `$TMPDIR`).
  A cheap middle: honour `$XDG_*` whenever they are set, on any platform,
  and fall back to `~/.solidrt/` otherwise. Linux users who care get correct
  behaviour, everyone else keeps one rule, and `devDir()` stays the single
  switch point.
- **Does the runtime participate?** Today srt passes `--data-root` and the
  client needs no knowledge of any of this. A split layout probably ends
  that, which turns a CLI-only change into a Rust one. Worth weighting
  heavily: the current arrangement is the reason the dev layout costs
  nothing to move.
- **Migration.** Existing `~/.solidrt/` trees would be orphaned: tickets
  rotate once as tunnel keys are regenerated, dev clients get new p2p
  identities, and installed dev apps need re-pushing. All dev-only and all
  disposable, so "just change it" is probably right, but a one-shot move of
  the old dir is cheap if it is not.
- **Does the same reasoning apply to the runtime side?** The launcher and
  packed apps resolve through `SDL_GetPrefPath`, which is not XDG-aware
  beyond landing in `~/.local/share`. That is
  `app-storage-vendor-path.md`'s territory, and the two should agree on
  whether an installed app's cache belongs in `XDG_CACHE_HOME`.

Related: `okf/backlog/parallel-dev-servers.md` (where `~/.solidrt/` came
from, and the Rejected entry this promotes to its own item),
`okf/backlog/app-storage-vendor-path.md` (the production half of the same
question), `okf/plans/client-storage-updates.md` (the tree shapes and their
original reasoning).
