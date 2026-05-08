# flux

An embeddable, extensible cross-platform JavaScript runtime in Rust built on [QuickJS-NG](https://github.com/quickjs-ng/quickjs) with a Tokio-based async event loop.

## Usage

```rs
use flux::FluxEngine;

let bytecode = std::fs::read("app.bin").unwrap();
FluxEngine::new().eval(bytecode).await;
```

For custom plugins and loggers, see the [examples/](examples/) directory.

### Logger

All console output flows through a `Logger`. By default, `console.log` writes to stdout and `console.warn`/`console.error` write to stderr. Use `.log()` on the builder to provide a custom handler that receives a `LogLevel` and message:

```rs
use flux::LogLevel;

let engine = FluxEngine::builder(runtime)
    .log(|level, msg| match level {
        LogLevel::Debug => { /* silenced */ }
        LogLevel::Log => println!("{msg}"),
        LogLevel::Warn => eprintln!("[WARN] {msg}"),
        LogLevel::Error => eprintln!("[ERROR] {msg}"),
    })
    .build();
```

### Evaluation methods

`FluxEngine` provides two ways to evaluate code. All evaluation runs as ES modules.

- **`eval(bytecode).await`** - loads and evaluates precompiled bytecode. Waits for all async work to complete.
- **`eval_source(code).await`** - evaluates JS source as an ES module (supports `import`/`export`). Requires the `compile` feature. Waits for all async work to complete.

```rs
// run precompiled bytecode
let bytes = std::fs::read("app.bin").unwrap();
engine.eval(bytes).await;

// run JS source (requires `compile` feature)
engine.eval_source(r#"console.log("hello")"#).await;
```

## Compiling to bytecode

Enable the `compile` feature to build the `flux` binary, which compiles JS source from stdin to bytecode on stdout:

```
cargo build --features compile
echo 'console.log("hello")' | ./target/debug/flux > app.bin
```

The `compile_source()` library function is also available behind this feature flag.

## Platform bindings

### I/O

`io.source(target)` creates a source object from a file path or HTTP URL.

```js
let src = io.source("data.json");   // file
let src = io.source("https://api.example.com/data");  // HTTP GET
```

The source object has three body methods, each returning a Promise. The body can only be consumed once (web `Response`-style):

```js
let text = await src.text();    // string
let bytes = await src.bytes();  // Uint8Array
let obj = await src.json();     // parsed JSON
```

The source also exposes its target as a `path` (file) or `url` (HTTP) property.

### Timers

```js
let id = setTimeout(cb, ms);
clearTimeout(id);

let id = setInterval(cb, ms);
clearInterval(id);
```

### Console

```js
console.log("info");     // print to stdout
console.warn("warning"); // print to stderr
console.error("error");  // print to stderr
```

## Building

```
cargo build                      # library only
cargo build --features compile   # library + compiler binary
cargo test
```

Requires Rust 2021 edition. Licensed under MIT.
