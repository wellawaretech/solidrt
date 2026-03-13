# qjsrt

A JavaScript runtime built on QuickJS-NG (via `rquickjs`) with a Tokio-based async event loop.

## Build & test

```
cargo build
cargo test          # integration tests in tests/
```

Tests use `run_script` with a 3s timeout. No special setup needed.

## Usage

```
qjsrt <file.js>            # module evaluation — waits for timers, no return value
qjsrt -e '<expr>'          # script evaluation — discard result
qjsrt -p '<expr>'          # script evaluation — print result
```

**Module flow** (`run` -> `JsEngine::eval`): evaluates as ES module via `Module::evaluate`. Errors go to stderr. Waits for all timers to drain, then exits.

**Script flow** (`run_script` -> `JsEngine::eval_script`): evaluates as global script via `ctx.eval`. Returns the stringified last expression. Waits for timers to drain before stringifying (so promises settle). `run_script` accepts an optional `Duration` timeout; CLI passes `None`.

## Constraints

- **JS values are `!Send`.** All JS execution must happen on the engine thread. Never move a `Value`, `Function`, or `Ctx` across threads.
- **Use `ctx.spawn()` for JS-touching async work**, not `tokio::spawn`. `ctx.spawn()` produces QuickJS-managed futures driven by `runtime.idle()`. `tokio::spawn` would require `Send` and break.
- **The engine thread runs a single-threaded Tokio runtime + `LocalSet`.** `spawn_local` is fine; `tokio::spawn` inside the engine thread would panic (no multi-thread runtime there).
- **PendingOps gates completion.** Both flows wait on `PendingOps::wait_idle()` before responding. Any async primitive that should keep the process alive must call `hold()`/`release()` on `PendingOps` (retrieved via `ctx.userdata()`).
- **Module flow vs script flow use different eval paths.** Module flow uses `Module::evaluate` (supports `import`/`export`). Script flow uses `ctx.eval` (returns a value). Don't mix them up.

## JS code style

- **`let` over `const`:** In JS examples and test code, always use `let`. Only use `const` for true constants named in `UPPER_CASE`. Never use `const` for ordinary bindings.

## Rust code style

- **Global registration pattern:** When registering JS functions on `globals`, define each `Function::new(...)` in its own `let` binding first, then group all `globals.set(...)` calls together at the end. Do not inline `Function::new` inside `globals.set`. See `timer.rs` `init_timers` for the reference pattern.
- **Async JS functions — named fn vs closure:** Use `Async(named_fn)` (named `async fn`) when the function returns a JS value (`Value<'js>`, `TypedArray`, etc.), because closures can't relate input/output lifetimes on `Ctx<'js>`. Use `MutFn` closures with `ctx.spawn()` when the function returns simple Rust types (`u32`, `()`) and manages JS values internally — no lifetime issue arises. See timer functions (closures) for examples.
- **Sync named fns returning JS values:** The same closure lifetime limitation applies to sync functions returning `Value<'js>`. Use a named `fn` when the return type contains a JS lifetime. See `io_source` (named sync fn returning `Value<'js>`) for the pattern.
- **`Promised<T>` for async methods on objects:** When a closure-based JS function needs to return a Promise, use `MutFn` closure returning `rquickjs::Result<Promised<impl Future>>`. Extract everything needed from `Ctx` synchronously (PendingOps, etc.), then wrap the async work in `Promised(async move { ... })`. This avoids lifetime issues since the future doesn't capture `Ctx`. Use `IntoJs` newtypes (e.g. `JsBytes`, `JsonValue`) to convert Rust types to JS values inside the promise resolution. See `io.rs` body methods for the reference pattern.
- **`PendingOps` via userdata:** `PendingOps` is stored in the context via `ctx.store_userdata()` at engine startup. Retrieve it with `ctx.userdata::<PendingOps>()` instead of passing it as a parameter. Any new async primitive that should keep the process alive must call `hold()`/`release()` on `PendingOps`.
- **Storing non-JS types in userdata:** `ctx.store_userdata()` requires `JsLifetime`. For external types (e.g. `reqwest::Client`), create a newtype wrapper with `#[derive(Clone, JsLifetime)]` and `#[qjs(skip_trace)]` on the inner field. See `HttpClient` in `io.rs`.

## Event emitter

`on(event, callback)` registers a JS listener and returns a numeric ID. `off(event, id)` removes it. From Rust, `engine.emit(event, data)` sends a JSON-encoded string that gets parsed into a JS value on the engine thread.

- **`emit()` data is JSON.** The `data` argument to `engine.emit()` is a `String` that must be valid JSON. It is parsed via `ctx.json_parse` on the engine thread; malformed JSON delivers `undefined` to listeners.
- **Command channel is bounded (32).** `emit()` uses `try_send` and silently drops the event if the channel is full. This means high-frequency emits from Rust can lose events. Callers that need delivery guarantees should not rely on `emit()` alone.
- **`PendingOps` lifecycle.** `on()` calls `hold()` when the first listener for an event name is registered. `off()` calls `release()` when the last listener for that event name is removed. Removing all listeners for all events allows the process to exit naturally.
- **Listener exceptions are swallowed.** JS errors thrown inside listener callbacks are currently silently discarded. Listeners should not rely on exceptions for control flow.
- **`emit()` returns nothing.** The caller has no way to detect whether an event was dropped due to a full channel. If delivery confirmation is needed, this API is not sufficient.

## Plugin system

`JsEngine::builder().plugin(fn)` accepts closures that receive `Ctx<'js>` and run during context initialization (after built-in globals). Plugins can store Rust state via `ctx.store_userdata()` and register custom JS globals. See `examples/plugin.rs` for the reference pattern.

**`eval_detached`** returns a `oneshot::Receiver<()>` immediately, allowing the calling thread to poll for completion instead of blocking. Useful when embedding the engine in a non-async context.

## Modules

- `main.rs` — CLI arg parsing, dispatches to `run` or `run_script`.
- `lib.rs` — Public API. Creates multi-thread Tokio runtime, instantiates `JsEngine`, drives eval + shutdown.
- `engine.rs` — Core engine. Dedicated thread, `tokio::select!` event loop (recv commands vs `runtime.idle()`), global setup, result stringification. `JsEngineBuilder` with `.plugin()` for extensibility.
- `events.rs` — `on(event, callback)`/`off(event, id)` globals and `emit_event` helper. `ListenerMap` stored in ctx userdata.
- `timer.rs` — `setTimeout`/`setInterval`/`clearTimeout`/`clearInterval` via `ctx.spawn()`. `Notify`-based `wait_idle`.
- `io.rs` — `io.source(target)` returns a source object synchronously (no Promise). Detects `http://`/`https://` URLs for HTTP (via reqwest with `qjsrt/<version>` user agent), otherwise treats as file path. A shared `reqwest::Client` is stored in ctx userdata via `HttpClient` wrapper. Body methods `.text()`, `.bytes()`, `.json()` return Promises and are single-consume (web-style).
- `examples/plugin.rs` — Example: extending the runtime with a custom `whoami()` global backed by userdata.
