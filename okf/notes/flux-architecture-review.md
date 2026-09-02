---
title: Flux architecture review
description: Layering holds and the engine loop is sound; the debt is transport protocol logic sitting in flux instead of forge, a per-frame protocol owned by the runner instead of flux::gui, duplicate JS-shape decoders in the gpu module, and a stretched userdata service locator.
created: 2026-09-02
---

# Flux architecture review

Architecture-focused read-through of the whole crate (~18k lines: engine,
three bins, three plugin layers, the shared toolkit) and of every lattice
call site into it, as of 2026-09-02. Companion to
[flux-crate-review.md](flux-crate-review.md) (2026-07-15), which covered
completeness, standards conformance and tests; those findings are not
repeated except where their status changed. Nothing was built or run.

## Verdict

The layering story is real and well documented: engine, three marshalling
layers named for what they marshal, forge cores. The engine loop is in good
shape: the Notify-based idle in `engine.rs` (`FluxEngine::run`) closes the
1 ms poll the July review flagged. The isolate design is clean: forge owns
link and protocol, flux owns thread, engine and promise routing, and the
kill switch dies with the parent context. `plugins/value.rs` states its
contract explicitly, and the fail-soft property decode landed.

The debt is concentrated in four places, ranked below.

## 1. Transport protocol logic lives in flux, not forge

`flux/Cargo.toml` pulls reqwest, hyper, http-body-util, bytes,
fastwebsockets, iroh and tokio-util directly - seven transport crates in a
marshalling crate. The symptoms:

- The WebSocket client (`standards_plugins/websocket.rs`, `run_client` and
  `run_writer`) hand-rolls the TCP connect, hyper handshake, frame loop,
  writer and close grace. The server's equivalents (`run_reader`,
  `run_writer`) already live in `forge::websocket`, so the client is the one
  protocol driver outside the core. It also carries a hand-rolled URL parser
  because there is no `URL` global (still open from the July review).
- `forge_plugins/websocket.rs::try_upgrade` rebuilds a hyper request and
  runs the fastwebsockets upgrade. Protocol, not marshalling.
- `forge_plugins/serve.rs::handle_request` decomposes the hyper request,
  strips the query, and builds the 405 with `Allow`. The route table is in
  forge; the dispatch policy is here.
- `standards_plugins/request.rs` stores a `hyper::upgrade::OnUpgrade`.
- `standards_plugins/fetch.rs::request_body_from_value` is public API
  returning a `reqwest::Body`; lattice's proxy consumes it through flux.
- `forge_plugins/p2p.rs::P2pStream::create` takes raw iroh
  `Connection`/`SendStream`/`RecvStream` because `forge::p2p::Endpoint`
  hands them back unassembled.

What done looks like: a forge client driver with a dispatch trait like the
server's `WsDispatch`; `try_upgrade` and the request-dispatch policy in
`forge::http`; `Endpoint::connect`/`accept_one` returning the forge
`Stream`; a forge body type in the fetch helper. The acceptance test is
mechanical: flux's `[dependencies]` lists forge, alloy, rquickjs, tokio, log
and taffy, nothing else.

## 2. The per-frame protocol is owned by the runner, not by flux

`alloy_plugins/mod.rs::install` is the single registration seam and
promises the runner never needs to know plugin order. Per frame that promise
does not hold: `lattice/src/runtime.rs` (`frame`) calls ten flux hooks in
an order flux's own doc comments require - stamp the tree clock, spatial
`stamp_clock`, `advance_players`, `camera::tick`, `video::tick`,
`gpu::tick`, `advance_virtual_time`, `raf::flush`, then the render event -
and mutates the render tree directly through the `SharedRenderTree`
userdata handle. A second embedder (the direct `render` export in
`tree.rs` explicitly invites one) would have to copy that sequence.

What done looks like: a `flux::gui::frame` module with two entry points.
One stamps clocks, advances players and transitions, ticks camera, video and
gpu, and returns whether a frame is demanded. The other advances virtual
time, flushes rAF and emits the render event. Lattice keeps the clock policy
and the paused-path decision. Ordering then lives where the plugins live
and is unit-testable.

## 3. Two decoders for the same JS shapes

Shader params and texture bindings are decoded once over `PropValue`
(`properties/mod.rs`: `decode_params`, `decode_texture_bindings`) and again
over raw rquickjs objects (`gpu.rs`: `collect_params`, `collect_textures`).
They already disagree: the gpu path skips `undefined` entries and requires a
non-negative integral id; the property path errors on `undefined` and
truncates any float. So a params write through a `<texture params>` prop and
through `setTargetParams` accept different input.

One decoder per shape, with the gpu module going through `to_prop_value`.
The same file shows the wider inconsistency in `alloy_plugins/`: `tree.rs`
and `gpu.rs` bind exports as captured closures inside `evaluate` (gpu's is
~800 lines); `spatial.rs`, `audio.rs` and `camera.rs` bind free functions
that read their state from userdata. The free-function style is the better
one and would halve `gpu.rs`.

## 4. Userdata as the service locator is stretched

123 `ctx.userdata::<T>()` lookups, 99 of them `expect`/`unwrap`, held
together by the fixed order in `plugins::init_context`. Within flux that is
workable. Three things are not:

- `EngineConfig` is stored whole and also split into `FetchCacheDir` and
  `UserAgent` (`engine.rs`, `build`). Fetch and http could read the config.
- `AlloyContext` is stored eight times: once bare for off-thread queries and
  once inside each of seven per-plugin state structs, most of which also
  carry the platform handle. One shared gui state plus per-plugin data would
  remove most of the cloning in `install`.
- Lattice reads `SharedRenderTree`, `AlloyContext` and `Timeline` out of
  flux's userdata in ten places (`runtime.rs`, `plugins/draw.rs`,
  `go/connection.rs`). Functions on `flux::gui`, the way `spatial::
  stamp_clock` already works, would let those types go crate-private. This
  is the prerequisite for point 2.

## 5. Event ownership is split by a fallback arm

`alloy_plugins/events.rs::forward` returns false for whatever flux does not
handle, and lattice's `Runtime::event` matches the rest. A new `AlloyEvent`
variant compiles cleanly on both sides and reaches nobody. An exhaustive
match in flux naming the runner-owned variants turns that into a compile
error.

## Smaller items

- `forge_plugins/events.rs` (the listener registry and sticky cache) has no
  JS surface; it is shared infrastructure and belongs under `plugins/` with
  the toolkit.
- Stale seam docs: several forge module headers (`process.rs`, `path.rs`,
  `subprocess.rs`, `websocket.rs`, `fs.rs`) still say "destined for the
  forge crate (see REDESIGN.md)" and cite `plugins/flux/*.rs` paths that no
  longer exist; `serve.rs` documents the handle as returned by `Flux.serve`;
  `flux/README.md` still lists `Flux.on`. The layering docs are otherwise
  the best in the repo, which makes the stale ones mislead more.
- `standards_plugins/body.rs::is_async_iterable` (and the sibling shims in
  `body.rs`/`marshal.rs`) `ctx.eval` a JS function on every call; cache it
  like `NativeQueueMicrotask` in `time.rs`.
- Still open from the July review: the three `examples/gpu_*.rs` use
  `flux::gui` with no `[[example]] required-features = ["gui"]` stanza, so a
  bare `cargo test -p flux` fails (root CLAUDE.md documents the workaround
  instead); subprocess, p2p, ffi and svg have no integration tests.
