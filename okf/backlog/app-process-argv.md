---
type: backlog-item
title: Apps cannot see their own arguments (lattice never sets ProcessArgs)
description: Resolved 2026-08-05 - argv is app arguments only, owned per app start; lattice installs ProcessArgs from the source-path tail, packed payloads own their whole command line, dev pushes carry the session's args, and exit() ends a playback run early.
status: done
tags: [flux, lattice, cli, render, scripting]
timestamp: 2026-08-05T00:00:00Z
---

# Apps cannot see their own arguments

## Resolution (2026-08-05)

Implemented, with one design change against the proposal below: argv is app
arguments only - no executable path, no script slot. The Node/Bun two-slot
prefix already diverged between hosts (flux put user args at index 2, fluxrt
at index 1, and the dev server keyed off "the last argument" to cope), and
lattice would have needed filler values for packed/launcher/dev apps.

Arguments belong to an app start, not the process. Who owns a command line:

- A distribution owns it whole (fluxrt parity): a packed binary - embedded
  trailer or adjacent folder, decided before flag parsing - passes everything
  after the executable to the app. Runner flags (`--size`, `--stats`, ...) no
  longer apply to packed binaries; they are dev tooling for the source-path
  shape.
- The dev runner shape (`solidrt-go [flags] source ...`): runner flags, then
  the source path, then the app's args verbatim (this also fixed the
  stray-flag-becomes-source-path bug). `srt render app.tsx -- x y` forwards
  the tail this way.
- The client owns its own line (`--dev-server`, `--data-root`, ...). Hosted
  apps are guests: a dev push carries the session's args (`srt run/server
  app.tsx -- x y` puts the tail in the server config, so remote clients see
  the same argv as local ones); a launcher launch passes an empty vector;
  the launcher and connect screen see empty argv.

The companion landed too: exit() during playback ends the process, so
`--duration` is an upper bound rather than a guess.

`flux:process` exports `argv`, filled from a `ProcessArgs` userdata the host
installs (`flux/src/plugins/modules/process.rs`). The `flux` and `fluxrt`
binaries both do it (`FluxEngine::builder().userdata(ProcessArgs(argv))`).
Lattice's builder chain (`lattice/src/lib.rs`, around the `flux::gui::install`
call) never does, so `argv` is an empty array in every solidrt app.

Verified: a probe app run under `srt render` printed `argv=[] platform=linux`.
`platform` and `arch` come through, only the argument vector is missing.

## Why it matters

An app has no channel for parameters at all. That is fine for a shipped
application and a real gap for the repo's own tooling: `scripts/changelog/`
renders the changelog to a PNG and is blocked on exactly this - the app can
capture itself and encode a PNG (`captureSnapshot` -> `readTexture` ->
`flux:image` `encodeImage`) but cannot be told where to write it.

Every alternative is worse:

- `import.meta` is empty after bundling - no `dir`, no `url` (verified).
- `flux:process` exposes no environment access, only argv/platform/arch.
- The runtime chdirs into the app's data sandbox before app code runs, so a
  fixed relative path lands somewhere the caller has to go hunting for.
- A generated module the driver writes for the app to import works today, at
  the cost of a generated source file the app cannot run without.

## Proposal

- Lattice sets `.userdata(ProcessArgs(...))` in the same builder chain that
  already installs the gui plugin set and the clock.
- `lattice/src/main.rs` collects everything after a bare `--` verbatim
  instead of parsing it. Worth doing on its own: today an unrecognised
  argument falls through to `source_path`, so a stray flag silently changes
  which app runs.
- The CLI forwards that tail for `render` and `run`.
- Absolute paths stay the caller's job. The runtime chdirs before app code
  runs, so a relative argument resolves inside the sandbox; document that
  rather than absolutising silently, which would corrupt arguments that are
  not paths.

## Decide while wiring it: what argv[0] is

The two sides disagree today. `flux/src/bin/flux.rs` passes
`std::env::args()` whole, so `argv[0]` is the executable and `argv[1]` the
script - what the Rust comments in `flux.rs` and `process.rs` both describe.
`packages/flux-types/modules/process.d.ts` instead documents `argv[0]` as the
script path with user arguments from `argv[1]`.

Keeping Node/Bun parity (executable, script, then user arguments) and fixing
the `.d.ts` is the smaller change and the less surprising contract. Whatever
is chosen, lattice should match flux exactly - two hosts with different argv
shapes is the worst outcome.

## Companion

For the capture-in-app workflow specifically, `exit()` should end a playback
run rather than restart the app: playback calls `std::process::exit(0)` when
its frame count runs out, so an app that writes a file has to outlive its own
async write by an arbitrary number of frames. That is a separate small fix in
the same area, and the two together turn the changelog tool into "capture,
encode, write, exit" with no frame-count guessing.
