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
- **Timer cleanup gates completion.** Both flows wait on `Timers::wait_idle()` before responding. If you add a new async primitive that should keep the process alive, it needs to integrate with this mechanism (or a similar one).
- **Module flow vs script flow use different eval paths.** Module flow uses `Module::evaluate` (supports `import`/`export`). Script flow uses `ctx.eval` (returns a value). Don't mix them up.

## Code style

- **Global registration pattern:** When registering JS functions on `globals`, define each `Function::new(...)` in its own `let` binding first, then group all `globals.set(...)` calls together at the end. Do not inline `Function::new` inside `globals.set`. See `timer.rs` `init_timers` for the reference pattern.

## Modules

- `main.rs` — CLI arg parsing, dispatches to `run` or `run_script`.
- `lib.rs` — Public API. Creates multi-thread Tokio runtime, instantiates `JsEngine`, drives eval + shutdown.
- `engine.rs` — Core engine. Dedicated thread, `tokio::select!` event loop (recv commands vs `runtime.idle()`), global setup, result stringification.
- `timer.rs` — `setTimeout`/`setInterval`/`clearTimeout`/`clearInterval` via `ctx.spawn()`. `Notify`-based `wait_idle`.
