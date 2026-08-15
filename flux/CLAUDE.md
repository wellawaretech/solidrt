# Flux

Flux embeds a JavaScript runtime built on QuickJS (via the `rquickjs` crate).
These conventions are flux-specific; the repo-wide rules in the root CLAUDE.md
still apply.

## Plugin layering

Plugins are thin FFI layers: marshal arguments and results between JavaScript and
Rust, nothing more. Domain logic belongs in the owning module (forge / alloy),
exposed as methods the plugin forwards to.

Three plugin layers under `flux/src/plugins/`:

- `standards/`: web-standard JS APIs installed as globals (`console`, `fetch`,
  the Fetch types `Headers`/`Request`/`Response`/`Body`, timers, `WebSocket`
  client, `TextEncoder`/`Decoder`).
- `modules/`: the `flux:*` capability modules, imported as `flux:http`,
  `flux:sqlite`, `flux:subprocess`, ... - marshalling over the forge capability
  cores. Put new flux-specific modules here.
- `gui/` (behind the `gui` feature): the alloy-backed render-tree and capture
  bindings. The runner (lattice) supplies the host instances via `gui::install`;
  flux owns which plugins exist and their registration order.

`js_error.rs` + `marshal.rs` + `value.rs` at the `plugins/` root are the shared
marshalling toolkit used across all three layers. `value.rs` is where
`forge::Value` meets JS: a forge result type implements `From<T> for Value` in
forge and the plugin returns `Neutral(result.into())`; do not hand-write a
per-type `IntoJs` for plain data results.

## Module surface

Prefer focused `flux:` modules over growing the `Flux.*` global. Web-standard
APIs stay global; flux features live behind `flux:` imports.

The `Flux` global is reserved for runtime introspection/metadata, not feature
APIs: `Flux.version`, `Flux.capabilities`. Capability checks branch on the
feature, not the OS (`Flux.capabilities.includes("subprocess")`); to test for an
external binary use `which` from `flux:subprocess`, not a platform check.

## Surface changes

Any change to what JS can see - a `decl.declare`/`exports.export`, a
`globals.set`, a `#[rquickjs::methods]` signature, or behavior a doc comment
describes - updates `packages/flux-types` (the matching `.d.ts` under
`modules/`/`standards/`/`gui/`) and `docs/flux.md` in the same change. Flux
projects compile with no lib.dom and no Node/Bun types: flux-types is the only
thing telling TypeScript what exists, and nothing verifies parity
automatically yet.

A new `flux:*` module additionally registers in `plugins/mod.rs` (resolver +
loader) and in `BASE_CAPABILITIES` (or `GUI_CAPABILITIES`), and its `.d.ts`
is referenced from flux-types `index.d.ts`.

## Threading model

All JS of one runtime runs on one thread; `flux:isolate` spawns further
runtimes, each on its own thread with its own heap, talking over ports that
carry `forge::Value` copies (see `modules/isolate.rs`). Within a runtime the
rules below hold unchanged. JS values (`Function`, `Object`, `Value`, any
`'js`-bound handle) are `!Send`; a future that touches one must be spawned
with `ctx.spawn`, which runs it on the JS executor. `tokio::spawn` is only for
pure native work whose future is `Send`; its results come back to JS by
resolving a `Promised` or via a channel a `ctx.spawn` task drains. The one way
into the engine from another thread is `ExecHandle::exec`, which queues a
closure the run loop executes with `Ctx` in hand (see `gui/events.rs` for the
pattern at scale).

## Ctx and the `'js` lifetime

A helper that builds and returns a `'js`-bound rquickjs value (`Object`, `Value`,
`Function`, `Array`, a `Class` instance) takes `Ctx<'js>` BY VALUE and returns
`...<'js>`, so the result's lifetime is the context's `'js`. Taking `&Ctx<'js>`
instead ties the result to the borrow, and since `Object` (and friends) is
invariant over `'js`, it will not unify with the caller's `'js` ("lifetime may
not live long enough"). See `build_file`, `build_dir`, `serve_impl`.

A plain fn item (like `build_file`) passed to `Function::new` gets the
`for<'js> Fn(Ctx<'js>) -> Result<Object<'js>>` HRTB for free. A CAPTURING
closure (needed when you must hold extra state) does NOT infer it - again
because `Object` is invariant over `'js` - and fails with "lifetime may not live
long enough". Force the HRTB with a coercion helper:

    fn object_builder<F>(f: F) -> F
    where F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<Object<'js>> { f }

    Function::new(ctx.clone(), object_builder(move |ctx| build_child(ctx, &spec)))

Do not put an explicit `-> rquickjs::Result<Object<'_>>` on the closure; that
pins two separate anonymous lifetimes and fails regardless. See
`flux:subprocess` `spawn` for the worked example.

## Persistent handles and classes

Never store a `Persistent` inside an rquickjs class. The class finalizer that
would release it runs during `JS_FreeRuntime`, after userdata is cleared, and
trips QuickJS's shutdown assertion (`gc_obj_list` not empty). Keep Persistents
in a context-userdata registry keyed by an id, and have the class hold ONLY
the id - a clone of the registry Rc kept in the class pins the Persistents
just as fatally. Userdata drops with the context, before the runtime is
freed, so everything releases in order. See `WasmHandlers` (flux:wasm) and
`FfiHandlers` (flux:ffi) for the worked pattern.

## Async results and errors

Fallible async work returns a `Promised` future whose `Output` is
`JsResult<T>` (from `crate::plugins::js_error`), wrapping a `Result<T, String>`.
The error message becomes a clean JS `Error` inside `JsResult`'s `IntoJs`.

Why not return `rquickjs::Error` from the future: a `Promised` future cannot
capture `Ctx` (the future outlives the closure's borrow), and
`rquickjs::Error::Io` renders to JS as `IO Error: ...`. Converting in `into_js`
runs on the JS thread with `ctx` in hand, so the rejection is a plain `Error`.

- Sync argument validation (where you already hold `Ctx`) throws directly with
  `Exception::throw_message(&ctx, msg)`.
- Hold/release `PendingOps` around the awaited work so the engine stays alive
  while the async op is in flight.

## Never panic on JS input

rquickjs does not catch panics in callbacks: a panic unwinds through QuickJS
and aborts the whole process. Anything a script can pass is untrusted input -
validate and throw (`Exception::throw_message`) or return `Err`, never
`panic!`/`expect` on it. `expect` is for infallible init-time work (building
registration objects, storing userdata), not for decoding values that arrived
from JS.

## Awaiting a JS promise natively

Before reading a JS-returned promise through `MaybePromise`/`into_future`,
mark it observed: `PromiseFuture::poll` takes a fast path for already-settled
promises that never attaches a reaction, so QuickJS's unhandled-rejection
tracker reports a rejection you are about to handle as if nobody looked at
it. Attach a real no-op rejection handler (a function) - NOT `undefined`,
since `.then(_, undefined)` yields a derived promise that re-rejects with the
same reason and is itself unhandled. See `mark_observed` in
`modules/serve.rs`.

## Fire-and-forget callbacks

When Rust invokes a JS callback with no JS caller above it (timers, event
listeners, rAF), report a throw with `report_uncaught` (`crate::logger`): it
pulls the pending exception off the context so the message and stack reach
the log instead of a bare "Exception generated by QuickJS". Never discard the
`Err` - that swallows the user's exception silently.