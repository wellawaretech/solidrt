# qjsrt

## Build & test

```
cargo build
cargo test          # integration tests in tests/
```

Tests run the `qjsrt` binary via `Command` (module mode through stdin). No special setup needed.

## Architecture

The engine is a generic "run closures on a JS thread" executor. It has no knowledge of module loading, bytecode, or evaluation modes.

**Key files:**

- `engine.rs` — `JsEngine`, `JsEngineBuilder`, generic event loop. Receives `JsCommand::Exec` (a boxed closure) and runs it on the JS context.
- `pending.rs` — `PendingOps`. Reference-counted async operation tracker. `hold()`/`release()` gates process completion.
- `plugins/mod.rs` — `init_context()` sets up the QuickJS runtime, module loaders, and all built-in plugins. Also defines the `PluginFn` type.
- `plugins/events.rs` — Both cross-thread `EventChannels` (Send+Sync buffering) and JS-thread listener dispatch (`on`/`off`/`emit_event`).
- `lib.rs` — Public API. Convenience functions (`run`, `run_bytecode`, `compile_source`) construct the engine and send the appropriate closure. Module-loading logic lives here, not in the engine.

**Caller-side evaluation:** `eval()` and `eval_bytecode()` are convenience methods on `JsEngine` that construct a closure containing the module-loading logic and send it via `exec()`. The engine event loop just runs the closure and waits for `PendingOps` to drain. Adding new eval modes requires no engine changes.

## Constraints

- **JS values are `!Send`.** All JS execution must happen on the engine thread. Never move a `Value`, `Function`, or `Ctx` across threads.
- **Use `ctx.spawn()` for JS-touching async work**, not `tokio::spawn`. `ctx.spawn()` produces QuickJS-managed futures driven by `runtime.idle()`. `tokio::spawn` would require `Send` and break.
- **The engine thread runs on the caller's Tokio runtime via `LocalSet`.** `spawn_local` is fine; `tokio::spawn` inside the engine thread would require `Send`.
- **PendingOps gates completion.** The engine waits on `PendingOps::wait_idle()` after running each closure. Any async primitive that should keep the process alive must call `hold()`/`release()` on `PendingOps` (retrieved via `ctx.userdata()`).

## JS code style

- **`let` over `const`:** In JS examples and test code, always use `let`. Only use `const` for true constants named in `UPPER_CASE`. Never use `const` for ordinary bindings.

## Rust code style

- **Global registration pattern:** When registering JS functions on `globals`, define each `Function::new(...)` in its own `let` binding first, then group all `globals.set(...)` calls together at the end. Do not inline `Function::new` inside `globals.set`. See `timer.rs` `init_timers` for the reference pattern.
- **Async JS functions — named fn vs closure:** Use `Async(named_fn)` (named `async fn`) when the function returns a JS value (`Value<'js>`, `TypedArray`, etc.), because closures can't relate input/output lifetimes on `Ctx<'js>`. Use `MutFn` closures with `ctx.spawn()` when the function returns simple Rust types (`u32`, `()`) and manages JS values internally — no lifetime issue arises. See timer functions (closures) for examples.
- **Sync named fns returning JS values:** The same closure lifetime limitation applies to sync functions returning `Value<'js>`. Use a named `fn` when the return type contains a JS lifetime. See `io_source` (named sync fn returning `Value<'js>`) for the pattern.
- **`Promised<T>` for async methods on objects:** When a closure-based JS function needs to return a Promise, use `MutFn` closure returning `rquickjs::Result<Promised<impl Future>>`. Extract everything needed from `Ctx` synchronously (PendingOps, etc.), then wrap the async work in `Promised(async move { ... })`. This avoids lifetime issues since the future doesn't capture `Ctx`. Use `IntoJs` newtypes (e.g. `JsBytes`, `JsonValue`) to convert Rust types to JS values inside the promise resolution. See `io.rs` body methods for the reference pattern.
- **`PendingOps` via userdata:** `PendingOps` is stored in the context via `ctx.store_userdata()` at engine startup. Retrieve it with `ctx.userdata::<PendingOps>()` instead of passing it as a parameter. Any new async primitive that should keep the process alive must call `hold()`/`release()` on `PendingOps`.
- **Storing non-JS types in userdata:** `ctx.store_userdata()` requires `JsLifetime`. For external types (e.g. `reqwest::Client`), create a newtype wrapper with `#[derive(Clone, JsLifetime)]` and `#[qjs(skip_trace)]` on the inner field. See `HttpClient` in `io.rs`.

## Event emitter internals

`on(event, callback)` registers a JS listener and returns a numeric ID. `off(event, id)` removes it. From Rust, `engine.emit(event, data)` takes a JSON string that is parsed into a JS value on the engine thread.

- **`emit()` data is a JSON string.** The caller is responsible for producing valid JSON. It is parsed via `ctx.json_parse` on the engine thread; malformed JSON delivers `undefined` to listeners.
- **Event channels use per-event FIFO buffers.** Registered via `event_channel()` on the builder. When full, the oldest event is dropped.
- **`PendingOps` lifecycle.** `on()` calls `hold()` when the first listener for an event name is registered. `off()` calls `release()` when the last listener for that event name is removed. Removing all listeners for all events allows the process to exit naturally.
- **Listener exceptions are swallowed.** JS errors thrown inside listener callbacks are currently silently discarded.
