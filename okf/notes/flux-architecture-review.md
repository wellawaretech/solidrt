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

**Status 2026-09-02: done, all three stages.** flux's direct dependencies
are forge, rquickjs, tokio and log (plus alloy and taffy behind `gui`).

- Stage 1, edge seams: `forge::fetch::Client` and `RequestBody` replace the
  reqwest client and body in flux's fetch (and lattice's dev-server proxy),
  `forge::p2p` hands back an assembled `Stream` plus `StreamWriter` (and a
  `connect_io` duplex for lattice's tunnel), `forge::subprocess::Spawned`
  yields `ByteStream`s, and the http/fetch channel bodies take `Vec<u8>`.
- Stage 2, client driver: `forge::websocket` owns `parse_ws_url`, the
  shared `ClientSocket` state (readyState, writer slot, close signal and
  the web API's close-code/reason validation), `run_client` (connect,
  handshake, frame loop) against a `ClientDispatch`, and the `ClientWriter`
  the host spawns. Its public surface uses a `Kind` enum (text, binary,
  ping, pong) instead of fastwebsockets' `OpCode`, and the writer queues are
  opaque (`SinkQueue`).
- Stage 3, server types: `forge::http` owns `RequestParts` (built from the
  hyper request inside `serve_connection`, which now takes an async
  `Fn(RequestParts) -> Reply` handler and the connection's `Remote`), the
  opaque `Reply` (`text`, `full`, `streamed`), and `UpgradeHandle`;
  `forge::websocket` owns `accept_upgrade` -> `Handshake` -> `PendingSocket`
  and the opaque `SocketRead`/`SocketWrite` halves the loops drive. The
  accept loops hand the peer to the host with the socket.

flux keeps what it should: the JS classes and handler properties, the
Request/Response marshalling, the routes-then-fetch dispatch policy (with
its 405 and 404), and every `ctx.spawn`. Verified: forge lib tests, the
flux integration suite (http, websocket client and server, fetch), the flux
gui unit tests, `cargo check -p lattice --features go`, clippy, and the
subprocess, p2p echo and p2p serve smoke scripts.

As found, `flux/Cargo.toml` pulled reqwest, hyper, http-body-util, bytes,
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

**Status 2026-09-03: done, both stages.** `flux::gui::frame` owns the order:
`advance(ctx, now_ms, period_ms)` stamps both animation clocks, advances
the clip players, ticks camera, video (feature gate inside flux) and gpu,
and latches the frame request itself when a device or player changed
content or still runs (the tree plugin already requests frames for its own
writes, so the note's sketched demand flag was not needed);
`deliver(ctx, frame, now_ms, timer_now_ms)` advances virtual time, flushes
rAF and emits the render event. The seven per-plugin hooks lattice called
are crate-private. Lattice's frame verb keeps input dispatch and the move
terminator, the clock policy, the speech pump (right after `advance`), the
pause gate with its native draw, the stepped-frame request and the timing
stamp. One measurement changed with it: the stamp now lands before
`deliver`, so the stats' JS figure (HUD `JS`, `jsMs`, the slow-frame line)
is the frame's JS - timers, rAF callbacks and the render handler - not the
handler alone; a heavy timer callback delays the frame, and now shows.
Verified: flux check, clippy and gui unit tests, lattice check, clippy and
unit tests, a release client, and on it `probes/timer-deadline-probe.tsx`
(all three phases: timers fire within a frame period in the idle and spin
phases and within the probe's own 40 ms frames in the heavy phase, which
is the slow-frame storm it is designed to induce), `probes/
transition-demo.tsx` under a frozen clock (the panel does not move after
the tap until frames are stepped, progresses per step, the tween ends
after its 21 frames and the color spring later, six transitionEnd lines
counting the three mount-time settles), and `examples/spin` at steady
state: 61 fps, 179 changed frames in 3 s, zero missed presents, zero slow
frames. The video tick is verified by compilation only: `examples/video`
does not build (the pre-existing `@solidrt/core/video` resolve error).

Stage 2: `frame::draw(ctx, extra_demand, |frame| ...)` runs the two
transition ticks and the demand gate in flux and hands the caller a
`Frame` whose `commit` resolves to `Reused` or a `Build` with `layout`,
`paint` and `finish`, each binding the tree borrow to its own call so JS
run between the phases can write properties. The draw bridge's closure
keeps its policy (overlay push, per-phase timing, the postLayout event,
hover refresh, stats and history; the node count comes from
`tree::node_counts`). Flux's driver is the one driver per tree: the direct
`render` export is a hook-less `draw`, lattice's second driver is gone, and
`tree::tick`, `spatial::tick` and `SharedRenderTree` are crate-private (the
input plugin is the handle's remaining reader). A nested `draw` from a JS
hook (a transitionEnd or postLayout handler calling `render`) finds the
driver taken and skips with a warning instead of panicking. Verified: flux
check, clippy and gui unit tests, lattice check, clippy and unit tests, the
three headless gpu examples (the direct path), a release client, and on it
`probes/transition-demo.tsx` under a frozen clock (the same per-step
timeline as stage 1, plus a node snapshot of the settled panel: the capture
interlock through the shared driver), `examples/text-flow` at steady state
(thirty synthetic moves, 33 rebuilt frames with the phase figures recorded,
zero slow frames, zero missed presents) and `examples/spin` (61 fps, 180
changed frames in 3 s, zero missed presents).

As found: `alloy_plugins/mod.rs::install` is the single registration seam and
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

**Status 2026-09-02: done, all three stages.** `properties::decode_params` and
`decode_texture_bindings` are the one implementation; `gpu.rs`
`collect_params`/`collect_textures` marshal the object through
`to_prop_value` and call them, so the prop and the imperative path accept
and reject identically: a null or undefined entry is skipped, a texture id
must be a non-negative integer, and a Float32Array/Float64Array is accepted
as a flat array on both (documented in flux-types `gpu.d.ts` and core
`types.d.ts`; core's `createShaderTextureMemo` record compare learned typed
arrays too). `gpu.rs` and `tree.rs` are in the free-function style: every
export is a `fn` reading the plugin state from userdata and `evaluate` is
one line per export (`gpu.rs` 1760 to 1588 lines, `tree.rs` 633 to 602; the
direct `render` export's `FrameDriver` moved into the shared
`RenderTreeInner`). `video.rs` needs no pass: its six closures are
per-player method bindings that capture only the player id and forward to
free `*_impl` functions, the shape `audio.rs`, `camera.rs` and
`microphone.rs` use for bound handles too; the alternative, an rquickjs
class per player, would change the JS shape for nothing. Verified: flux gui
unit tests (two new: undefined entries skipped end to end through the
marshaller, strict ids on both binding forms), the three headless gpu
examples (`gpu_manual`'s stale `setShaderParams`/`setShaderTextures`
imports fixed in passing), lattice check, clippy, `srt check` (only the
pre-existing `@solidrt/core/video` resolve error remains), and
`gpu-shared-params.tsx` plus 3d `lit.tsx` on the dev client: shared and
per-entry params and bindings in the GPU inventory, no warnings, zero
missed presents. Stage 3: the same tests, examples (they draw through the
direct `render` export), lattice check and clippy, plus `examples/text-flow`
on a release dev client: prepareText with carets, layoutNextLine breaks and
mixColors in the tree, a pointer move re-breaks the lines around the circle
into left and right slots, zero missed presents, and no log entry beyond
one first-paint slow-frame report.

As found: shader params and texture bindings are decoded once over `PropValue`
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

**Status 2026-09-02: done, stages 1 and 2; stage 3 deferred to finding
2.** Stage 1: the fetch and http initializers read `cache_dir` and
`user_agent` off the stored `EngineConfig`; `FetchCacheDir`, `UserAgent`
and their two store closures in `build` are gone. Stage 2: lattice no
longer reads flux userdata. `flux::gui::tree` gained `stamp_clock`, `tick`
(advance the tracks, emit the transitionEnd events, return frame demand),
`node_counts` and `with_tree`; `flux::gui::alloy_context` hands the runner
the `Arc<alloy::Context>` for its dev-server queries (capture, GPU
inventory, texture and buffer reads, raster counters); `flux::
timeline_now_ms` is public for the playback clock. `AlloyContext` is
crate-private, with the seven `store_state` functions: lattice's draw
bridge and speech plugin hold the plain `Arc<alloy::Context>` (lattice
owns the instance and already hands it to `GuiHost` as one).
`SharedRenderTree` stayed public for exactly one reader, the draw bridge's
frame build, until finding 2 stage 2 moved that build into flux; it is
crate-private now. Stage 3 (2026-09-03): one shared `Gui` state (the alloy
context and the platform handle), stored once by `install` before any
plugin; the `AlloyContext` newtype and every per-plugin handle copy are
gone. A plugin state with data of its own (tree, gpu, camera, video) holds
the shared state as `gui: Rc<Gui>` - gpu and video need the alloy handle
in Drop, where their teardown releases textures, buffers and sinks - and
spatial, microphone and audio, which held nothing but the handle, have no
state and no `store_state`; rAF reads the platform at init. `install` is
one store plus plain `store_state` calls. Lattice is untouched: `install`,
`alloy_context` and the frame module keep their signatures. Verified:
flux check (gui and video), clippy and gui unit tests, lattice check,
clippy and unit tests, the three headless gpu examples, and on a release
client `probes/transition-demo.tsx` under a frozen clock, `examples/spin`
at steady state and `examples/audio` (the audio plugin's state changed
shape). Camera and microphone are compile-verified only: the headless
client has no devices. Stage 1 and 2 verification:
flux check, clippy and gui unit tests, the http, isolate and engine
integration suites (an isolate child inherits the config), lattice check,
clippy and unit tests, and on a release dev client:
`probes/transition-demo.tsx` under a frozen clock (tap, step, the tween
read mid-flight and settled through the tree query, the three transitionEnd
handler logs, node counts and raster figures in the stats reply, a node
snapshot), then `gpu-shared-params.tsx` for the GPU inventory, a texture
read, a vertex buffer read (the quad's corners) and the missing-id error;
no warnings, zero missed presents at steady state.

As found: 123 `ctx.userdata::<T>()` lookups, 99 of them `expect`/`unwrap`, held
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

**Status 2026-09-02: done, stage 1.** `forward`'s wildcard is an explicit
arm naming the eight runner-owned variants (the four pointer events, the
two frame signals, Quit, Exposed), so the match is exhaustive: a probe
variant added to `AlloyEvent` fails flux's build with E0004 at that match.
Lattice's `Runtime::event` keeps its wildcard on purpose (the flux error
already forces the ownership decision; a second list would duplicate the
sixteen marshalled variants). Verified: flux check, clippy and gui unit
tests, lattice check.

As found: `alloy_plugins/events.rs::forward` returns false for whatever flux does not
handle, and lattice's `Runtime::event` matches the rest. A new `AlloyEvent`
variant compiles cleanly on both sides and reaches nobody. An exhaustive
match in flux naming the runner-owned variants turns that into a compile
error.

## Smaller items

- `forge_plugins/events.rs` (the listener registry and sticky cache) has no
  JS surface; it is shared infrastructure and belongs under `plugins/` with
  the toolkit.
- Stale seam docs (fixed 2026-09-02): several forge module headers still
  said "destined for the forge crate (see REDESIGN.md)" and cited
  `plugins/flux/*.rs` paths that no longer existed; `serve.rs` documented
  the handle as returned by `Flux.serve`; `flux/README.md` listed `Flux.on`.
  All corrected; the README's `Flux` section now states the
  introspection-only contract.
- `standards_plugins/body.rs::is_async_iterable` (and the sibling shims in
  `body.rs`/`marshal.rs`) `ctx.eval` a JS function on every call; cache it
  like `NativeQueueMicrotask` in `time.rs`.
- Still open from the July review: the three `examples/gpu_*.rs` use
  `flux::gui` with no `[[example]] required-features = ["gui"]` stanza, so a
  bare `cargo test -p flux` fails (root CLAUDE.md documents the workaround
  instead); subprocess, p2p, ffi and svg have no integration tests.
