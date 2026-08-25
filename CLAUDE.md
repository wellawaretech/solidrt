# Code style
ASCII characters only. No em-dashes.

User-facing messages (CLI output, errors, logs) are sentence-case: capitalize the first word. Keep it consistent across commands.

## JavaScript and TypeScript
Prefer `let` over `const`. Use `const` only for "real" constants of a single value referred to in ALL_CAPS.

This project uses SolidJS 2.0. For the reactivity, control-flow, and props model (props are reactive values not accessors, no destructuring, no top-level reactive reads, etc.) consult `node_modules/solid-js/CHEATSHEET.md` - it is the authoritative reference.

## Rust
Never only use `.unwrap()`; use `.expect(..)` or `.unwrap_or(..)` or something similar to explicitly handle the scenario where the result is not Ok.

Plugins (the `*/plugins/` modules that register `ffi`/global functions) should be thin FFI layers: marshal arguments and results between JavaScript and Rust, and nothing more. Domain logic belongs in the owning module (e.g. rendertree), exposed as methods the plugin closure forwards to.

The rendertree must stay engine-independent: no QuickJS/`rquickjs` (or any JavaScript) references. It should be usable from other engines, not even necessarily JavaScript. JS value parsing belongs in the plugin layer; rendertree methods take and return native Rust types only.

`flux/src/` has three plugin layers, named for what they marshal: `standards_plugins/` for web-standard JS APIs (e.g. `fetch`, `Response`, `Headers`, `console`, timers) whatever backs them, `forge_plugins/` for the `flux:*` capability modules (e.g. `serve`, `file`, `sqlite`) that marshal forge cores, and `alloy_plugins/` (behind the `gui` feature, exported as `flux::gui`) for the alloy-backed render/capture bindings. A web standard goes in `standards_plugins/`; otherwise the crate marshalled decides. `flux/src/plugins/` holds only the shared marshalling toolkit (`js_error.rs`/`marshal.rs`/`value.rs`) and context setup.

# API design
Look at standard (web) functionality through a solidrt lens: solidrt runs a single known application, not the whole internet. Keep the standard names and shapes, but simplify the semantics to what an app needs, and document the simplified contract plainly.

API input validation: throw in dev, ignore-with-warning in prod. No runtime dev/prod signal exists yet (see `okf/backlog/dev-prod-validation-policy.md`), and everything running today is dev, so validation sites just throw.

# Dependencies
## SDL
SDL is accessed through the sdl3 Rust crate, which does not expose all SDL functionality. If something is not available in the sdl3 crate, check if it's available in SDL directly, and if so, add a wrapper function in `alloy/src/sdl_utils.rs`.

# Projects
## Rust
- `alloy` platform and rendering: SDL, Impeller, glow (GL), and the rendertree (layout, hit testing, compositing)
- `forge` engine-free capability cores (HTTP, fs, sqlite, subprocess, p2p, ...); no scripting-engine types
- `flux` embeds a JavaScript runtime built on QuickJS; its plugins marshal the forge cores and the alloy GUI
- `lattice` the SolidRT runtime: binds alloy, forge, and flux and drives the event loop

## JavaScript
- `packages/core` SolidRT core, linking SolidJS and Lattice
- `packages/cli` SolidRT command-line developer tooling 

# Building
Run from repo root:
- `make client` - build the go client binary (release)
- `make client PROFILE=debug` - build the go client binary (debug)
- `make runtime` - build the production runtime (release)

Debug builds are for functional verification only: unoptimized Rust (and
QuickJS especially) is drastically slower, so never quote or record
performance timings from a debug client - rebuild release first. Counters
and other behavioral measurements are profile-independent.

# Running an app for verification (dev server + MCP)
Never use the built-in `run` skill here. Drive the app yourself:

- Start: `(bunx srt run <entry.tsx> > <scratchpad>/run.log 2>&1 &)` from repo
  root. `srt run` starts the dev server AND a local client; with no terminal
  on stdin it runs without the repl and stays up on its own (it logs "No
  terminal on stdin"). Give it ~10 s, then check
  `~/.solidrt/servers/34884/live.json` exists (default port 0x8844;
  `-s <N>` = port + N).
- Stop: signal the srt process by pid. `pkill -f "srt run <entry.tsx>"` also
  matches the shell that ran it, so `pgrep -af "<entry.tsx>"`, pick the
  `bun`/`bunx` pid, and `kill` that - it tears down the server and the
  client. The live.json is removed on exit; a leftover record means a crash.
- MCP tools (`mcp__solidrt__*`) resolve the server by PROJECT: the bridge's
  cwd (repo root here) must match the served entry's nearest package.json.
  An entry under `packages/<pkg>/` or `examples/<x>/` registers THAT
  directory as projectDir, so the repo-root bridge reports "No dev server"
  even though one is running. In that case talk to the control API
  directly with curl on `http://127.0.0.1:<port>/__control__/...`; the
  endpoints and response shapes are documented in
  `packages/cli/src/server/agents.md` ("The control API without MCP"). The
  bridge is a long-lived process: a change to `packages/cli/src/mcp/main.ts`
  only takes effect in a re-spawned bridge.
- Verify through the tree, not by eye: `send_input`/`/input` tap real
  coordinates from `/tree`, then read state back from the tree, a snapshot,
  or `/logs`. Snapshot the smallest node, not the window root.

# Notes (okf/)
Long-lived notes live under `okf/`, one markdown file per item. Write new notes there instead of scattering them across root scratch files, and check `okf/backlog/` before starting speculative or non-trivial work.

Read `okf/README.md` before adding one. The directory a document sits in is its state, so there is no status field and things move by `git mv`; `okf/index.md` is generated by `scripts/build-okf-index.ts` and must never be hand-edited.

# Versioning
Every package/crate version in source is the `0.0.0` placeholder, including the intra-monorepo `@solidrt/*` deps in `packages/cli/src/init/scaffold/package.json`. The `.github/workflows/release.yml` action bumps all of these to the real version and pins the intra-monorepo deps at publish time. So a scaffolded project's `bun install` fails in-repo (nothing on npm matches `0.0.0`) but works against the published packages. Do not "fix" the `0.0.0` placeholders in source.

# General
We are a team. Ask questions if input is not clear. Ask for me to try something if that is much easier. Do not over-engineer, go for minimalistic first.

If you get a prompt which asks to implement something, but there's a non-trivial reason why that is not easy, then point this out and ask for feedback how to continue.

Always ask for user confirmation of your plan before starting to implement.

Prefer staged proposals where the first stage is always to do the bare minimum, while focussing on elegant, correct code. 

If you get a question without asking for an implementation, then just answer the question instead of implementing anything.

If you need to think a lot, give short intermediary status updates what you are doing.