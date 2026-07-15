---
type: analysis
title: CLI package review - completeness, quality, tests
timestamp: 2026-07-15T00:00:00Z
---

# CLI package review - completeness, quality, tests

Full review of `packages/cli` (~2.8k lines of TypeScript: 1.9k in the srt/Bun
process incl. 8 commands, 0.9k in the spawned flux dev server, plus scaffold
templates and the bin stub) as of 2026-07-15. Every source file read; the
`--help` crash verified by running the binary. Companion to the forge, alloy,
and flux crate reviews. Context: okf/backlog/cli-flux-migration.md plans to
fold the Bun side into flux eventually, so findings note which side they land
on.

## Summary

The package is small, coherent, and unusually well-commented for CLI glue: the
three-process architecture (srt/Bun owns bundling + repl + watcher, a spawned
flux script owns HTTP/WS/proxy/tunnel, solidrt-go is the client) is deliberate,
documented at every seam, and dogfoods flux as its own dev server. The pure
core (`bundleWith`, `remapPositions`, the cache decision logic, `packRunner`)
is cleanly separated from ambient state, and error messages are consistently
actionable (build hints per binary, adb install instructions per OS).

The gaps are of the "tooling for the tooling" kind rather than architectural:
there are zero tests of any form, `srt --help` crashes with a raw TypeError
stack trace instead of printing usage, the README documents flags that do not
exist, and the repl's `reload`/`load` commands only handle `.tsx` while every
other path accepts `.tsx/.jsx/.ts/.js`. Nothing found rises to
data-loss-or-corruption severity; the watcher's unguarded rejections and
missing debounce are the closest to real runtime defects.

## Completeness

The command set (init, run, server, client, bundle, render, pack, mcp) covers
the intended dev loop end to end, including Android install/launch with
subnet-matched dev-server address resolution, the p2p ticket tunnel, capture/
playback, and an 11-tool MCP bridge. Boundaries and holes:

- **No `--help`/`-h` and no `--version`.** Worse, any unknown flag escapes
  `parseArgs` (strict mode) as an uncaught TypeError with a Bun stack trace -
  verified: `srt --help` dumps `ERR_PARSE_ARGS_UNKNOWN_OPTION` pointing at
  args.ts:3. Bare `srt` prints good usage; the crash path just needs a
  try/catch around `parseArgs` plus a help option.
- **README.md is stale and thin.** It documents `srt run --server [file]` and
  `srt run --client`, neither of which exists (both would hit the TypeError
  above); the real spelling is `srt server` / `srt client`. It also omits
  init, bundle, render, pack, mcp, --tunnel, and --capture entirely. AGENTS.md
  by contrast is accurate, current, and the best doc in the package; the
  README could largely defer to it.
- **Hardcoded port.** `DEV_PORT = 0x8844` with no `--port`, so one dev server
  per machine; a second instance dies as "[cli] Dev server exited unexpectedly
  (1)" rather than a clear port-in-use message.
- **`srt client` cannot be pointed at a server** - the `--dev-server`
  pass-through is a live TODO (commands/client.ts:18). Standalone clients rely
  on QR/recents only; manual entry is a known pending item.
- **Host coverage**: binary resolution knows linux-x64, darwin-arm64,
  win32-x64 only. linux-arm64 and darwin-x64 hosts get "build it from source"
  with no triple mapping even for the SRT_HOME path (artifacts.ts TRIPLE_MAP).
- **`--output` is a basename, not a filename**, despite the help text saying
  "Output filename": `-o app.js` writes `app.js.srt.js`. Either honor an
  extension when given or rename the help text.
- **`render` leaves its intermediate `<entry>.srt.js` behind** next to the
  source, every run.
- **init edge cases**: the folder-must-not-exist check does `readdir(dir)` and
  treats any failure as "does not exist", so a plain *file* at the target path
  slips through to a messy mkdir/copy failure. The `dir === "."` branch in the
  final "Next:" hint is unreachable ("." always exists and is rejected
  earlier). Non-TTY stdin silently scaffolding `solidrt-app` with the default
  template is a documented, reasonable choice.

## Code quality

Production-level in the ways that matter for a dev tool, with a handful of
genuine defects:

- **Repl extension inconsistency (defect).** `srt server app.jsx` bundles fine
  (isSource covers 4 extensions) and the watcher rebuilds on all 4, but the
  repl's `reload` only rebuilds when the source ends in `.tsx` (repl.ts:44) -
  for a `.jsx`/`.ts` entry it silently re-sends the stale latched bundle. And
  `load` rejects `.jsx` outright ("Unsupported file type"). One shared
  predicate (the args.ts ones, exported for paths not just the launch arg)
  would fix all three sites.
- **Watcher has no debounce and no rejection guard (defect).** `fs.watch`
  fires multiple events per save, each triggering a full bundle concurrently;
  completion order is not guaranteed, so a stale build can be latched after a
  fresh one (last-finished-wins race). And the async callback awaits
  `sendReload` without the `guard()` treatment the repl gives every command:
  a failed server round-trip mid-watch becomes an unhandled rejection, which
  is process-fatal under default Bun/Node semantics. A short debounce +
  in-flight coalescing + guard is ~10 lines.
- **`splitQuery` can throw on malformed input** (server/main.ts:50-53):
  `decodeURIComponent("%")` throws URIError, and any LAN peer reaches this
  before any route handling. Wrap or tolerate.
- **Capture chain poisoning**: `state.captureChain =
  state.captureChain.then(append)` - one rejected append (disk full, file
  deleted) leaves the chain rejected forever, silently dropping every later
  capture event and leaking an unhandled rejection. Needs a `.catch` that
  logs and continues.
- **MCP server version is hardcoded `"0.0.0"`** (mcp.ts:223) while server.ts
  correctly reads `pkg.version` (which the release workflow bumps). The MCP
  bridge will report 0.0.0 in published builds.
- **`bonjour-service` is a shipped dependency with zero live call sites** -
  all uses are commented out (mDNS advertise, deliberately kept as future
  scaffolding). Keep the comments, but the dependency itself costs every
  install; it can be re-added with the code.
- **Minor**: dev-client.ts:22 falls through after `shutdown()` (benign only
  because shutdown exits synchronously); dev-android.ts reads a child's piped
  stderr after awaiting exit (theoretical pipe-buffer deadlock on huge
  output); `print()` emits `\r\x1b[K` even when stdout is not a TTY (control
  chars in redirected logs); pack.ts shells out to `chmod` where
  `fs.chmodSync` would do; cli/tsconfig.json still includes
  `default-app/**/*`, a directory that no longer exists.

Strengths worth naming so they survive refactors: the NODE_ENV re-exec dance
in main.ts and the sourcemap "serve each map exactly once" trick in
composeMap are both subtle and thoroughly explained at the site; `bundleWith`
is kept pure precisely so the bundle-cli subprocess and the in-process path
cannot drift; the server-side rebuild goes through the same bundle-cli for the
same reason; `/__internal__/` is correctly loopback-gated; the QR renderer
documents *why* every ANSI choice exists.

## Security posture

Consistent with the decided default-accept LAN model, but one asymmetry is
worth an explicit decision: the file routes give every LAN peer unauthenticated
**read of the whole sourceDir (with directory listings) and PUT write into
it**, regardless of `--proxy-files` (that flag only tells clients to route
their fs calls through the server; serving is unconditional).
`/__control__/` (rebuild-and-push, debug command invocation into the app,
GPU/texture readbacks) is likewise LAN-open; only `/__internal__/` checks the
peer. AGENTS.md warns about `--proxy-files` "exposes your dev machine's
files", but the exposure exists without the flag. Options, cheapest first:
gate PUT (and perhaps directory listing) behind `proxyFiles`, or restrict
file/control routes to loopback + tunnel peers unless a flag opens them.

## Tests

**There are none.** No test files, no test script in package.json, nothing.
This is the only package/crate in the repo with zero coverage, and it is the
tool every user touches first. The good news is the codebase is already
factored for it - these are pure and testable today with `bun test`, no
process spawning needed:

- `remapPositions` (remap.ts): the 1-based/0-based column dance, `main:L`
  without column, malformed-map fallback - exactly the kind of logic that
  regresses silently.
- `splitQuery` (server/main.ts): would have caught the URIError.
- Cache decisions (cache.ts): `shouldConsider`/`isBypass`/`hasNoStore` header
  parsing.
- The Range header parsing in `handleFiles` (worth extracting for it):
  suffix ranges, clamping, 416 cases.
- `binaryImport` (bundler.ts) via `transformAsync` on fixture strings.
- `hostIpFor`/`ipToInt` (dev-android.ts): pure subnet math.
- `packRunner` trailer layout (packer.ts) against a fixture buffer.
- `packageName`, `resolveBinary`/`resolveApk` with a temp SRT_HOME.

A second tier (still cheap, one spawned process): boot server/main.ts with a
synthetic config, assert `/__internal__/clients` answers, non-loopback 403s
are... untestable locally, but the file routes, latch/replay-on-connect, and
control 404s all are. Since server/ is already a flux script, these double as
flux integration tests - they survive the planned cli-into-flux migration,
which is an argument for starting there.

## Improvement points, ranked

1. Add tests, starting with the pure modules above (an afternoon; highest
   regression-protection per line anywhere in the repo right now).
2. Catch `parseArgs` errors into usage output; add `--help`/`--version`.
3. Unify entry-extension handling across server command, repl `reload`,
   repl `load`, and the watcher (fixes the stale-reload defect for .jsx/.ts).
4. Debounce + coalesce + guard the watcher callback.
5. Rewrite README.md (remove the nonexistent flags, list all commands, defer
   detail to AGENTS.md).
6. Decide the LAN PUT/control exposure question explicitly.
7. Fix the capture-chain poisoning and `splitQuery` throw.
8. `--port` flag (or at least a clear port-in-use message).
9. Drop `bonjour-service` from dependencies; read MCP version from
   package.json; honor or re-document `-o`.
10. Small hygiene: stale tsconfig include, render's leftover `.srt.js`,
    init's exists-as-file check.
