---
title: Flux crate review
description: Marshalling contract upheld, error model strong; the two biggest 2026-07-15 gaps - gui prop panics and fetch silently dropping Headers/bodies - are fixed as of 2026-08-06 (see status note). Remaining gaps are standards-conformance nits and missing teaching examples.
created: 2026-07-15
---

# Flux crate review

## Status update 2026-08-06

The top two improvement points landed:

1. **Fail-soft gui property decode (point 1): done.** All value-decode panic
   sites in `plugins/gui/properties/` (and `to_prop_value`'s UTF-8 expect)
   return `Err` through apply_jsx's existing channel; a bad JSX value throws
   a catchable JS Error naming property, value, and accepted set. The gui
   layer's first unit tests exist (`flux/src/tests/properties.rs`, 10 tests
   driving apply_jsx), closing the "properties/ decode is pure and directly
   unit-testable" half of the gui test gap. The five copy-pasted string-throw
   helpers (camera/microphone/audio/gpu/tree) now throw real `Error` objects
   via `Exception::throw_message`.
2. **fetch correctness (point 4): done** ("Fixed Flux silent fetch drops",
   2026-08-06) - Headers instances are read via the shared
   `header_pairs_from_init`, unsupported body types throw, direct tests added.

Point 3 (W3C key values, repeat flag, console Error formatting) had already
landed separately (`alloy/src/keymap.rs`, KeyEvent modifier fields,
`format_error` in console.rs). The rest of the list stands.

Full-crate review of `flux` (~9.7k lines of source: engine + 3 bins + 3 plugin
layers; 51 plugin files) as of 2026-07-15: every layer read (all of the engine,
standards, and modules; gui sampled across tree/properties/raf/texture/events),
the integration suite run (129 tests in 16 files, all pass, ~2.5s), clippy run.
Companion to the forge and alloy reviews; forge findings are not repeated here
except where the flux layer adds its own angle.

## Summary

Flux is production-level for its current scope and the best-documented crate in
the repo. The marshalling-only contract ("plugins are thin FFI layers") is
upheld in every module; domain logic genuinely lives in forge/alloy. The error
model is uniform and unusually well worked out - `with_pending`/`JsResult` for
async rejections, `report_uncaught` for fire-and-forget callbacks, and an
unhandled-rejection tracker with HTML-spec microtask-checkpoint semantics that
most embeddings do not bother to get right. The hard rquickjs problems
(invariant `'js` lifetimes, the Persistent-in-class GC trap, promise-observed
marking) are each solved once, documented in flux/CLAUDE.md or at the site, and
reused consistently.

The weak spots are concentrated, not diffuse: the gui property decoders panic
on bad property values (one typo'd JSX value aborts the process), the
`gui_hello` example breaks `cargo test -p flux` outright, `fetch` silently
drops a `Headers` instance passed as the headers option, the engine idles on a
1ms poll loop, and the same three capability modules that are untested in forge
(subprocess, p2p, ffi) are also untested here - plus wasm and the whole gui
layer.

## Completeness

The engine surface (builder with plugins/userdata/module
overrides/logger/stack size, `ExecHandle` for cross-thread injection, shutdown
hooks, `PendingOps` liveness, bytecode compile/eval) covers what lattice and
the standalone bins need. The three bins (`flux`, `fluxc`, `fluxrt` with
appended-bytecode trailer) are minimal and correct. Boundaries worth knowing:

- **No `URL`/`URLSearchParams`.** The largest missing web standard, and it is
  already being paid for: `standards/websocket.rs` hand-rolls ~50 lines of URL
  parsing (with its own IPv6-bracket handling) because there is no URL type to
  lean on. Query-string handling in serve routes is a manual `split('?')`.
- **No fetch cancellation** (`AbortController`/`signal`), and no
  `redirect`/`timeout` options. Fine for current consumers; will be asked for
  the moment someone streams a long download.
- **fetch headers option only accepts a plain object.** Passing a `new
  Headers(...)` instance type-checks as an `Object`, but its entries live in
  Rust, not in JS props, so the key iteration sees nothing and the request goes
  out with NO headers, silently. Given Headers is a global we provide, this is
  a trap of our own making.
- **fetch body values that are not string/Uint8Array/async-iterable are
  silently dropped** (body becomes None). A plain object should either throw
  or JSON-encode; silence is the worst of the options.
- Deliberate and documented: no `ReadableStream` (async-iterables are the
  house pattern), `wss://` rejected until TLS lands, `Headers` iterates via
  `forEach` only, WebSocket client has handler properties but no
  `addEventListener`. The sqlite module doc's Bun-divergence list is the model
  for how to document this kind of scope decision.
- **Timer web-compat nits**: `setTimeout(cb)` without a delay throws (the arg
  is a required `u64`; web/Node default to 0), and extra arguments are not
  forwarded to the callback. `crypto.getRandomValues`/`randomUUID` and
  `structuredClone` are absent - each turns up early in ported JS.
- The `flux:*` module set (sqlite, fs, http, p2p, net, mdns, process, path,
  subprocess, wasm, ffi + the gui five) is coherent, and `Flux.capabilities`
  correctly reflects the build (gui names appended only under the feature).
- **No teaching examples.** `packages/core/examples/` is a curated,
  README-indexed set of 20 single-concept "copy one and adapt it" apps; flux
  has nothing equivalent. `flux/examples/*.js` are 13 manual smoke scripts
  (half literally named `*_test.js`), unindexed, covering only fs/sqlite/
  subprocess/wasm/serve/websocket/p2p - no net, mdns, process, path, ffi, or
  fetch/streaming-body examples, and nothing showing cross-module patterns.
  `docs/flux.md` is a reference, which answers "what exists" but not "how do
  I do X the flux way". Concrete recurring case: getting binary data into a
  program. The answer is split across layers - `with { type: "binary" }` is
  a BUNDLER feature (packages/cli, shown in core's `binary-import.tsx`);
  plain flux ignores import attributes, where the answer is
  `file(path).bytes()` (shown only in passing inside `file_io.js`) - and no
  flux-level example or doc states the split, so it gets re-derived every
  time it comes up.

### Standards conformance (audited specifically)

The recurring "X behaves differently than the web" experiences have one root
pattern: flux borrows web VOCABULARY (`key`, `code`, `shiftKey`, `fetch`
options, `Headers`) without always delivering web VALUES/BEHAVIOR, and nothing
marks the line. Where a divergence is documented (sqlite's Bun list, Headers'
forEach comment, the ws:// stage note) there has been no confusion; every
reported surprise has come from an undocumented one. Found by this audit:

- **Keyboard events are the biggest offender.** `emit_key` (gui/events.rs)
  passes SDL names straight through as `key`/`code`, so against the W3C UI
  Events values: `key` is `"Return"` not `"Enter"`, `"Left"` not
  `"ArrowLeft"`, `"Space"` not `" "`, and letters are always uppercase
  (`"A"`) where the web is case-aware (`"a"`/`"A"` by shift); `code` is
  `"A"`/`"Left Shift"` where the web says `"KeyA"`/`"ShiftLeft"`. No JS layer
  normalizes: core's window.ts forwards the raw event, and
  components/text-input.tsx already codes against SDL names - including the
  tell-tale hedge `e.key === "Return" || e.key === "Enter"`. Every new
  component author pays this again, and the cost of converging on W3C names
  grows as more consumers absorb SDL naming.
- **`repeat` is dropped at the alloy boundary**: SDL's KeyDown carries a
  repeat flag, `AlloyEvent::KeyDown` discards it, so JS cannot distinguish
  auto-repeat - a standard `KeyboardEvent.repeat` field apps routinely need.
- **`console.log(new Error("x"))` prints `{}`**: the formatter is
  JSON.stringify-based, and Error's message/stack are non-enumerable. The
  web/Node print name, message, and stack. This one actively hurts debugging.
  (Circulars and functions degrade to `"[object]"`; no `%s`-style specifiers,
  no info/trace/group - acceptable scope, but the Error case is a trap.)
- The fetch/Headers, `setTimeout`, and Headers-iteration deviations listed
  above are the same pattern on the network side.

The event-bus dispatch model itself (srt:events + focused-node key delivery,
no DOM bubbling for keys) is a deliberate house design and not at issue; the
problem is only field values that look web-standard but are not. The repo
already has the right precedent: `gui/properties/` declares "every frontend
convention lives here" and translates JSX/CSS vocabulary to native values at
the marshalling layer. `emit_key` is the same kind of seam and should do the
same translation (SDL -> W3C names), one `match` away.

### flux-types parity (audited specifically)

`packages/flux-types` is very close to complete and its quality is exemplary:
every one of the 25 module exports, all 33 gui functions, the standards
globals, and the `Flux` global are declared; member-level surfaces (file(),
Server, WebSocket, sqlite) match the Rust implementations; and divergences
are documented in the doc comments per the house policy (the fetch.d.ts
"deliberate subset" banner, time.d.ts noting the required delay and
unforwarded args). Three defects found:

- **`atob`/`btoa` are missing entirely** - the runtime installs both
  (standards/base64.rs); no `standards/base64.d.ts` exists, so TS rejects
  code the runtime happily runs.
- **time.d.ts claims an inverted divergence**: clearTimeout/clearInterval
  say "Throws on an unknown id", but the runtime deliberately no-ops
  (matching web/Node, per its own comment). The types document a divergence
  that does not exist.
- **fetch.d.ts encodes the Headers bug as OK**: `HeadersInit =
  Record<string, string> | Headers` and `RequestInit.headers?: HeadersInit`,
  so TypeScript actively approves passing a `Headers` instance to `fetch` -
  the exact call the runtime silently sends with no headers. Whichever way
  the runtime bug is resolved, the type must match.

The larger point: nothing keeps this package honest. Parity today is pure
discipline - no test fails when a `decl.declare` is added without a `.d.ts`
edit (atob/btoa shows discipline alone eventually misses). The surface is
mechanically enumerable from both sides, so this is automatable: see
improvement point 5.

## Code quality

Production level. Specifics:

- **Layering discipline is real.** Every module opens with "Marshalling only:"
  and means it; forge types cross the boundary through `IntoJs` newtype
  wrappers (`JsRows`, `JsResponseData`, ...) with the orphan-rule rationale
  stated each time. Spawning stays in the marshalling layer, engine-free cores
  never see `ctx.spawn`.
- **The rquickjs hard parts are handled once and documented**: the
  `for<'js>` HRTB coercion helper for capturing closures, the
  Persistent-in-class shutdown-assertion trap (wasm and ffi both use the
  userdata handler registry, with the reason written at both sites),
  `mark_observed` in serve (a genuinely subtle QuickJS fast-path interaction,
  explained in full at the site).
- **The gui property decoders panic on bad values.** `apply_jsx` returns
  `Err` for an unknown property NAME ("a single typo'd prop must not take
  down the runtime", per its own comment) but ~35 `panic!`/`expect` sites
  fire for a bad property VALUE: an unknown enum string
  (`flexDirection="colum"`), a non-number color, a 3-element radius array.
  A panic unwinds across the rquickjs callback boundary and aborts the
  process. Related: `to_prop_value` does
  `s.to_string().expect("property string must be valid UTF-8")` - a JS string
  with a lone surrogate (possible from user text input) panics the same way.
  This is the known "fail-soft decode" open item; now that gui lives in flux
  it is this crate's biggest robustness hole, and the decoders are pure
  functions, so fixing and unit-testing them needs no GPU.
- **`unwrap`/`expect` usage is disciplined in practice**: nearly all sites are
  context-init or plugin-registration time (infallible object/function
  creation), where aborting is the right move. A few bare `.unwrap()`s on
  userdata lookups in events.rs/time.rs would be `.expect("...")` under the
  house rule, but they are init-guaranteed; cosmetic only.
- **The engine idle loop polls.** In `FluxEngine::run`, once the job queue
  drains and pending ops remain, `runtime.idle()` resolves immediately on the
  next loop iteration, so the select degenerates into a yield + 1ms sleep
  spin: a headless server sitting idle wakes ~1000x/s. Waking only on
  exec/pending/job-enqueue signals would make long-running flux processes
  power-clean.
- **Client WebSocket writes are unbounded** - the writer queue is an
  unbounded mpsc "without the backpressure/drain accounting" (its own
  comment) that the server side has. Same class of issue as forge's p2p
  writer; a fast producer against a slow server grows memory without bound.
- **`unsafe` is minimal and justified**: 12 sites - the bytecode `Module::load`
  (inherent), hand-written `JsLifetime` impls with the reasoning stated, and
  length-checked `slice::from_raw_parts` over TypedArray raw parts in the gui
  texture/audio/camera paths (single-threaded, copied out immediately).
- **Broken example target**: `flux/examples/gui_hello.rs` uses `alloy` and
  `flux::gui` but has no `[[example]]` entry with
  `required-features = ["gui"]`, so `cargo test -p flux` and
  `cargo clippy -p flux --all-targets` fail before running anything. The
  suite only runs with `--tests`. One Cargo.toml stanza fixes CI-ability.
- **Clippy is essentially clean** on the headless lib: two `redundant_closure`
  in engine.rs and a missing `Default` for `FluxEngine`. The gui-feature
  clippy run is blocked by alloy's `not_unsafe_ptr_arg_deref` errors (an
  alloy finding), so gui code has never actually been linted.
- **Docs drift in one place**: `docs/internals/flux.md` links to a
  nonexistent `../reference/flux.md` (the real page is `docs/flux.md`), shows
  a `FluxEngine::builder(runtime)` signature and `.log()` method that do not
  exist (`builder()` / `.logger()`), and attributes bytecode compilation to
  the `flux` binary (that is `fluxc`). The user-facing `docs/flux.md` is
  current by comparison. The plugin API TODO at the bottom is still open.

## Tests

129 integration tests in 16 files (~2.9k lines), all passing in ~2.5s, all
gated on `feature = "compile"` (signals additionally unix-gated). The harness
(`tests/common`: LogSink + run_source + TempDir) is small and pleasant, and
assertion style is behavioral (observed log output through the real engine),
which exercises the full marshalling path. Coverage by area:

| area | tests | verdict |
|---|---|---|
| engine lifecycle, promises/rejections, timers | engine 5, promises 9, time 22 | strong - the rejection-checkpoint semantics are pinned by tests |
| console, web_api (Headers/Request/Response/TextEncoder), base64 | 9 + 12 | good |
| http serve (routes, streaming, errors, ws upgrade) | http 17, websocket 3 | good |
| fs (file/dir/write), path, sqlite | 7+3+4+9+13 | good |
| net, mdns, events, process (signals, argv) | 4+3+3+6 | good |
| fetch | none dedicated | acceptable - exercised as the client in every http test, but only against loopback; the Headers-instance bug above would have been caught by one direct test |
| **subprocess** | **none** | gap (manual `examples/subprocess.js` only) - same gap flagged in the forge review, so it is untested at BOTH layers |
| **wasm** | **none at flux layer** | forge has 11 unit tests, but the marshalling (BigInt i64, host-function dispatch, exception rethrow) is its own risk surface; `examples/wasm_test.js` is already the test, unpromoted |
| **p2p, ffi** | **none** | gap at both layers; ffi is the riskiest module in the stack |
| **gui (all of it)** | **none** | tree/raf/texture need an alloy host, but `properties/` decode + `to_prop_value` are pure and directly unit-testable - and that is exactly where the panics live |

## Improvement points, ranked

1. **Fail-soft gui property decode**: convert the ~35 value-decode panics in
   `plugins/gui/properties/` (and the UTF-8 expect in `to_prop_value`) to the
   `Err(String)` path `apply_jsx` already has for unknown names, and add unit
   tests for the decoders (pure functions, no GPU). One bad JSX value must not
   abort the app.
2. **Gate `gui_hello`** with an `[[example]]` + `required-features = ["gui"]`
   stanza so `cargo test -p flux` and `--all-targets` work again.
3. **Normalize key events to W3C values** in `emit_key` (SDL -> `Enter`,
   `ArrowLeft`, `" "`, case-aware `key`, `KeyA`-style `code`) and thread
   SDL's `repeat` flag through `AlloyEvent::KeyDown`. Do it now, while only
   text-input.tsx has to migrate; every later consumer raises the cost. Fix
   the console Error formatting (print message + stack) in the same pass.
4. **fetch correctness**: accept a `Headers` instance for the headers option
   (or throw on it); throw on unsupported body types instead of silently
   sending nothing. Add the missing direct fetch tests while there.
5. **flux-types: fix the three defects and automate parity.** Add
   `standards/base64.d.ts`, correct the clearTimeout/clearInterval claim,
   align the fetch headers type with whatever fetch actually accepts. Then
   make drift fail a test: check a `surface.json` manifest into
   packages/flux-types as the meeting point - a flux integration test
   enumerates the real runtime surface from JS (import each module,
   `Object.keys` + typeof; globals; class prototype members) and asserts it
   matches, and a small script on the types side extracts declared names
   from the `.d.ts` files (TS compiler API) and diffs the same manifest.
   Either side changing without the manifest turns red and names the symbol;
   updating the manifest becomes the explicit act of changing the surface.
6. **Add `URL`/`URLSearchParams`** as a standards module; collapse the
   hand-rolled ws URL parser onto it.
7. **Promote the manual example scripts to integration tests**: subprocess
   (echo/cat fixtures), wasm (wat fixtures), ffi smoke, p2p pure parts -
   `examples/*.js` shows the suite already knows how; this closes the
   both-layers-untested hole flagged in the forge review too.
8. **Build a flux examples set modeled on `packages/core/examples/`**:
   single-concept, README-indexed, one per module plus the recurring
   cross-cutting patterns (binary data in - bundler import attribute vs
   `file().bytes()` - streaming bodies, capability checks). The existing
   smoke scripts are seed material; pairs with point 6 (the same fixtures can
   back both the examples and the missing integration tests).
9. **Fix the engine idle poll**: wake on job-enqueue/exec/pending signals
   instead of the 1ms sleep, for headless long-running processes.
10. **Fix `docs/internals/flux.md`** (broken reference link, wrong builder API,
   fluxc mixup); document the plugin API while there.
11. Client WebSocket write backpressure (pairs with forge's p2p item).
12. Web-compat nits when convenient: optional `setTimeout` delay + forwarded
   callback args, `crypto.getRandomValues`/`randomUUID`, `structuredClone`,
   WebSocket `addEventListener`. Track as scope items, not debt.
13. Clippy trivia (2 redundant closures, `Default` for `FluxEngine`) - and
    unblock gui linting by fixing alloy's `not_unsafe_ptr_arg_deref` errors.

## flux/CLAUDE.md follow-up (applied 2026-07-15)

The review fed a pass over flux/CLAUDE.md. Added: the surface-change rule
(flux-types + docs/flux.md in the same change; new-module registration
checklist) and five trap sections - threading model (ctx.spawn vs
tokio::spawn, ExecHandle), never panic on JS input, the Persistent-in-class
GC trap, marking natively-awaited promises observed, and report_uncaught for
fire-and-forget callbacks.

Considered and deliberately NOT added, recorded here so the option is not
lost (add if the file's scope should grow from traps to conventions):

- **Testing conventions section**: integration tests drive real JS through
  `tests/common` (run_source/LogSink/TempDir), one file per module, gated
  `#![cfg(feature = "compile")]`, assertions on captured log lines; examples
  needing the gui feature get `required-features` in Cargo.toml.
- **Web-name = web-value norm**: a JS-visible field borrowing a web-standard
  name must carry the web-standard value, or the divergence is documented in
  the flux-types doc comment. As much a core/components policy as a flux one,
  so it may belong at repo level instead.
- **PendingOps granularity** (extend "Async results and errors"): hold for a
  standing resource's lifetime (server accept loop, first listener / last
  removal), but only across each in-flight read for pull-based iterators, so
  an abandoned iterator cannot wedge the engine.
- **Streaming house pattern** (one line in "Module surface"): byte streams
  are async-iterables built with the `marshal.rs` helpers, not
  ReadableStream.
