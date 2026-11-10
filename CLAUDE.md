# Never name an outside project
HARD RULE, NO EXCEPTIONS. Nothing checked into this repository names or points at a project that uses SolidRT: an app, a game, a client's product or the maintainer's own, private or public. Not its name, its path, its package or bundle id, its identifiers, its assets, its screenshots, or code and tuning values copied from it. This holds for every file - source, comments, tests, examples, docs, `okf/`, AGENTS files, generated output - and for commit and PR text.

A problem found in such a project is described by its shape ("a title whose letters animate") and reproduced here from scratch, with neutral names and neutral numbers. Where it was seen is never recorded: not as provenance, not in a test comment, not in a note.

Third-party software, dependencies and prior art (Chrome, Godot) are not this; name them as before.

Before finishing any change made while looking at an outside project, search the working tree for that project's name and paths (`grep -rIil <name>`, outside `node_modules` and `target`) and remove every hit. A hit that was already there is removed too.

# Code style
ASCII characters only. No em-dashes.

No magic numbers, in every language here (TypeScript, Rust, GLSL): a tuning literal (threshold, ratio, blend width, epsilon, budget) is a named ALL_CAPS constant with a comment on what it tunes. Plain geometry (a midpoint's 0.5, a diameter's 2) stays inline.

User-facing messages (CLI output, errors, logs) are sentence-case: capitalize the first word. Keep it consistent across commands.

## JavaScript and TypeScript
Prefer `let` over `const`. Use `const` only for "real" constants of a single value referred to in ALL_CAPS.

This project uses SolidJS 2.0. For the reactivity, control-flow, and props model (props are reactive values not accessors, no destructuring, no top-level reactive reads, etc.) consult `node_modules/solid-js/CHEATSHEET.md` - it is the authoritative reference.

When writing solidrt application code (examples, scratch apps, repo tools, scaffolded projects), follow the instructions in the scaffold template `packages/cli/src/init/scaffold/AGENTS.md`. Read it before writing app code: it is the authoritative model for host elements, props, reactivity traps, assets, and headless verification.

## Rust
Never only use `.unwrap()`; use `.expect(..)` or `.unwrap_or(..)` or something similar to explicitly handle the scenario where the result is not Ok.

Plugins (the `*/plugins/` modules that register `ffi`/global functions) should be thin FFI layers: marshal arguments and results between JavaScript and Rust, and nothing more. Domain logic belongs in the owning module (e.g. rendertree), exposed as methods the plugin closure forwards to.

The rendertree must stay engine-independent: no QuickJS/`rquickjs` (or any JavaScript) references. It should be usable from other engines, not even necessarily JavaScript. JS value parsing belongs in the plugin layer; rendertree methods take and return native Rust types only.

`flux/src/` has four plugin layers, named for what they marshal (and a fifth, `test_plugins/`, for `flux:test`: behind the `test` feature, on the `flux` binary only, over flux's own facilities): `standards_plugins/` for web-standard JS APIs (e.g. `fetch`, `Response`, `Headers`, `console`, timers, `navigator`) whatever backs them, `forge_plugins/` for the `flux:*` capability modules (e.g. `serve`, `file`, `sqlite`) that marshal forge cores, `ore_plugins/` for the `flux:*` modules that marshal ore's OS facilities (e.g. `flux:link`), and `alloy_plugins/` (behind the `gui` feature, exported as `flux::gui`) for the alloy-backed render/capture bindings. A web standard goes in `standards_plugins/`; otherwise the crate marshalled decides. `flux/src/plugins/` holds only the shared marshalling toolkit (`js_error.rs`/`marshal.rs`/`value.rs`), the event bus mechanism (`events.rs`, no JS surface of its own) and context setup.

## Documentation
Write the prose and headings in `docs/` in ASD-STE100 Simplified Technical English: approved words, short sentences, one instruction per sentence, active voice. Product, command and API names stay as they are (`sol pack` is "pack"). The landing page (`docs/index.md`) is exempt.

# API design
Look at standard (web) functionality through a solidrt lens: solidrt runs a single known application, not the whole internet. Keep the standard names and shapes, but simplify the semantics to what an app needs, and document the simplified contract plainly.

API input validation: throw in dev, ignore-with-warning in prod. No runtime dev/prod signal exists yet (see `okf/backlog/dev-prod-validation-policy.md`), and everything running today is dev, so validation sites just throw.

# Dependencies
Every Rust dependency requirement is an exact `=x.y.z` pin, in the workspace
table and in every member manifest. Cargo.lock does not hold a version still on
its own: `cargo update` floats a caret requirement to any semver-compatible
release. Do not relax a pin back to `"1"` or `"0.4"`; bump it deliberately, and
never leave a pre-release string in place once the final release is out.

Reliability decides a dependency, not its implementation language and not
FFI avoidance: adopt only battle-tested, maintained software. For a
capability backed by a proven native library, the order is (1) a
battle-tested, maintained safe binding crate used through its safe API, (2)
FFI bindings we write ourselves against the proven library, in one thin
`unsafe` module with a safe type above it, pinned to the library's ABI
version. Never a young, single-publisher, unmaintained, pre-release, or
fetch-at-build-time crate, however safe its Rust looks; "pure Rust, no C" is
not a reason to adopt unproven software. Do not hand-write FFI to reach
beneath the safe API of a crate we already trust (e.g. rquickjs's raw `qjs`).

## SDL
SDL is the OS abstraction library, and `ore` is the one crate that names it for new work (alloy's existing SDL code moves there as features touch it). ore reaches SDL through the sdl3 Rust crate, which does not expose all SDL functionality. If something is not available in the sdl3 crate, check if it's available in SDL directly (`sdl3::sys`), and if so, wrap it in the ore module of its facility. `alloy/src/sdl_utils.rs` takes no new wrappers.

# Projects
## Rust
- `ore` the OS layer, the platform boundary: every OS facility, one contract each (today the main-thread bridge, opening a link and the locale). It wraps SDL where SDL reaches and our own native code (JNI, Win32, Objective-C, D-Bus) where it stops, and depends on nothing of ours. Read okf/design/platform-layering.md before placing any platform or OS feature
- `alloy` the window's scene and its rendering: the renderer and raster thread, GL (glow) and Impeller, the rendertree (layout, hit testing, compositing), the spatial core and frame timing (`FrameRelease`, cadence, refresh counting, liveness). alloy is facts-based: it reports facts and carries out what lattice chooses; policy is lattice's. It still holds the SDL window, the event pump and the device subsystems (audio device, camera, clipboard, file dialogs) and the Android glue; they move to ore as features touch them
- `forge` portable capabilities, the same code on every platform (HTTP, fs, sqlite, subprocess, p2p, decoders, ...): it calls ore where a capability touches the OS, and names no scripting-engine types. Its existing OS code (content providers, the AMediaCodec decoder's JNI side, the tty) moves to ore as features touch it
- `flux` embeds a JavaScript runtime built on QuickJS; its plugins marshal the forge cores, the ore facilities and the alloy GUI
- `lattice` the SolidRT runtime: binds alloy, forge, ore and flux and drives the event loop; it reaches ore for host concerns only (init order, app identity, storage roots, the windowless host), never for app features
- `runtime` the stock program over lattice (crate `solidrt`): the `solidrt` and `solidrt-go` binaries and the Android cdylib, embedding the player and BSOD bundles from `packages/cli/player/` (generated by `make player-bundle`); built by `packages/cli/runtime/runtime.mk`, the base its one-line Makefile includes, as a custom runtime (this crate with modules, `sol init --runtime`) does from its node_modules

Placement, in order (okf/design/platform-layering.md):

1. Does it reach the OS or SDL - anything per platform, any `cfg(target_os)`? ore.
2. Is it the scene, rendering, or frame timing? alloy: the facts and their execution, never the policy choice.
3. Is it the same code on every platform? forge.
4. Is it how an app is hosted, or a policy choice over alloy's facts? lattice.

New work that reaches SDL or the OS goes into ore; nothing new of that kind
lands in alloy, forge or lattice, and their existing OS code moves to ore
when a feature touches it, not before. "Engine-free" and "headless" do not
decide it: ore has no window part yet, and headless `flux` links it. An
app-facing facility reaches JS through flux `ore_plugins/`, never through
lattice. A feature with two halves (a dialog shows in alloy, the picked
document reads in forge) puts each half where its question says, and the
halves meet on a plain value (a location string, a NativeWindow, decoded
PCM), never on a trait one crate exposes for another to implement behind a
global. On Android, alloy publishes the JavaVM and the activity once in
ndk-context at startup; forge and ore read them there.

## JavaScript
- `packages/core` SolidRT core, linking SolidJS and Lattice
- `packages/cli` SolidRT command-line developer tooling
- `packages/router` screens as routes: a typed route tree, a stack, links in and the location out; a layer on core, core knows nothing of routes

A package's AGENTS.md ships to app developers' agents: it carries the
model, the traps every user hits, and an index of its `agents/*.md` topic
files, each with a "Read before <work>" trigger. A topic's depth and its
own traps go in `agents/`, not in AGENTS.md.

# Building
Run from repo root:
- `make client` - build the go client binary (release)
- `make client PROFILE=debug` - build the go client binary (debug)
- `make runtime` - build the production runtime (release)
- `cargo test -p flux --lib --features gui` - the flux property/plugin tests; a bare `cargo test -p flux` builds the gpu examples without `gui` and fails
- `cargo test -p flux --lib --features test` - the same, with the flux:test module's tests
- `cargo test -p flux --test <name>` - one of the cargo integration tests in `flux/tests/*.rs`, which hold only what Rust observes about an engine (the logger, uncaught reporting, idleness, exit, the embedding API); each is a binary linking flux, so run them one at a time. The flux module tests are `flux/tests/*.test.ts`, run by `sol test`
- `bun run sol check` - the typecheck: at the repo root it bundles and typechecks every entry (example apps, package examples, demos and test files, the CLI's command modules); with a path, that one entry. Typecheck with this, never with a raw `tsc -p <tsconfig>`, which reports errors `sol check` does not (a config that only makes sense under the bundler's settings)
- `bun run sol test` - the JS tests (`tests/*.test.ts`): flux tests on the `flux` binary (`make -C flux flux`), app tests (files that import `@solidrt/test`) on the dev client in its headless test mode (`make client`)

Debug builds are for functional verification only: unoptimized Rust (and
QuickJS especially) is drastically slower, so never quote or record
performance timings from a debug client - rebuild release first. Counters
and other behavioral measurements are profile-independent.

# Running an app for verification (dev server + MCP)
Never use the built-in `run` skill here. Drive the app yourself:

- Start: `(bun run sol run <entry.tsx> --project --port <N> > <scratchpad>/run.log 2>&1 &)`
  from repo root. The repo root is itself a project, so a bare file path is
  refused: `--project` serves the entry as the repo-root project (assets,
  isolates, MCP resolution), `--file` serves the file on its own. Pick a
  `--port` well above 34884 (say 34899) so it cannot collide with the
  user's own servers, which take the first free port from 34884 up. `sol
  run` starts the dev server AND a local client; with no terminal on stdin
  it runs without the repl and stays up on its own. Give it ~10 s, then
  check the log for `Connected to ws://127.0.0.1:<port>`. The registry is
  keyed by project, not port: the record is
  `~/.solidrt/servers/<key hash>/live.json` (fields: pid, port, key, mode,
  entry, projectDir); `cat ~/.solidrt/servers/*/live.json` lists every
  server with its port. One server per project: a second `--project` run
  while the user's repo-root server is up is a clash, not a second server.
- Stop: `curl -X POST http://127.0.0.1:<port>/__control__/shutdown` - the
  server acks `{ok: true}` and exits orderly (kills its local client,
  removes its live.json, closes the listeners). Address it by the port YOU
  started it on; never shut down a server the user started. Fall back to
  signalling the sol process by pid only if the server is wedged and the
  curl gets no answer: `pgrep -af "<entry.tsx>"`, pick the `bun`/`bunx`
  pid, and `kill` that - it tears down the server and the client. The
  live.json is removed on exit; a leftover record means a crash (the next
  server start prunes it, and resolvers confirm a record against the
  server before using it).
- The self-match trap, which bites often: any `pgrep -f` / `pkill -f` whose
  pattern is spelled out in the command that runs it also matches that
  shell, because the pattern is part of the shell's own command line. A
  `kill $(pgrep -f "sol run <entry.tsx>")` (or a `for p in $(pgrep ...)`
  loop) then kills the shell it runs in - the tool call dies with exit 144
  and nothing is torn down. Prevent it by bracketing the first character of
  the pattern: `pgrep -f "[s]rt run <entry.tsx>"`. The regex `[s]rt` still
  matches the real `sol` process, but never the literal `[s]rt` in its own
  command line. Never `pkill -f`.
  Never kill a server the user started (their `sol run` shows up in `pgrep`
  too, e.g. `apps/console`).
- MCP tools (`mcp__solidrt__*`) resolve the server by PROJECT: the bridge's
  cwd (repo root here) must match the record's projectDir, which a
  `--project` run sets to the repo root. A `--file` run has no projectDir,
  so the bridge reports "No dev server" even though one is running. In
  that case (or whenever the bridge is not wired) talk to the control API
  directly with curl on `http://127.0.0.1:<port>/__control__/...`; the
  endpoints and response shapes are documented in
  `packages/cli/agents/debugging.md` ("The control API without MCP"). The
  bridge is a long-lived process: a change to `packages/cli/src/mcp/main.ts`
  only takes effect in a re-spawned bridge.
- Verify through the tree, not by eye: `send_input`/`/input` tap real
  coordinates from `/tree`, then read state back from the tree, a snapshot,
  or `/logs`. Snapshot the smallest node, not the window root. Between the
  action and the read, wait with `settle`/`/settle` (the app at rest:
  nothing in flight, no frame demanded), never with a sleep.
- The control API is bigger than this summary. Animated app: `POST
  /clock?scale=0` freezes the frame clock, `?step=<n>` steps it - freeze,
  step, snapshot; never time snapshots against an animation. GPU
  internals: `/gpu?label=&draw=` is the resource inventory (per-draw
  counts, uniforms, formats), `/texture?id=` reads any texture back as
  `{ width, height, pngBase64 }` (JSON, not a PNG body);
  "does X draw?" is a drawCount read, not a lighting experiment. Seed
  state with debug commands (`registerDebug` from `sol:dev`, POST
  `/debug`). Before any verification beyond a static snapshot - and
  before concluding a tool for something does not exist - read
  `packages/cli/agents/debugging.md` in full, not via grep.

# Notes (okf/)
Long-lived notes live under `okf/`, one markdown file per item. Write new notes there instead of scattering them across root scratch files, and check `okf/backlog/` before starting speculative or non-trivial work.

Read `okf/README.md` before adding one. The directory a document sits in is its state, so there is no status field and things move by `git mv`; `okf/index.md` is generated by `scripts/build-okf-index.ts` and must never be hand-edited.

# Audit
When asked to audit solidrt, check every rule in `AUDIT.md`.

# Versioning
Every package/crate version in source is the `0.0.0` placeholder, including the intra-monorepo `@solidrt/*` deps in `packages/cli/src/init/scaffold/package.json`. The `.github/workflows/release.yml` action bumps all of these to the real version and pins the intra-monorepo deps at publish time. So a scaffolded project's `bun install` fails in-repo (nothing on npm matches `0.0.0`) but works against the published packages. Do not "fix" the `0.0.0` placeholders in source.

# General
We are a team. Ask questions if input is not clear. Ask for me to try something if that is much easier. Do not over-engineer.

If you get a prompt which asks to implement something, but there's a non-trivial reason why that is not easy, then point this out and ask for feedback how to continue.

Always ask for user confirmation of your plan before starting to implement.

Propose the complete feature, focussing on elegant, correct code.

If you get a question without asking for an implementation, then just answer the question instead of implementing anything.

If you need to think a lot, give short intermediary status updates what you are doing.