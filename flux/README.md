# qjsrt

An embeddable, extensible cross-platform JavaScript runtime in Rust built on [QuickJS-NG](https://github.com/quickjs-ng/quickjs) with a Tokio-based async event loop.

## Usage

```rs
use qjsrt::run;

fn main() {
    let code = r#"
        console.log('hello, world!')
    "#;

    run(&code);
}
```

## Advanced usage

Use the builder to extend the runtime with custom globals backed by Rust state:

```rs
let engine = JsEngine::builder()
    .plugin(move |ctx| {
        ctx.store_userdata(MyState::new()).unwrap();

        // register JS functions that read from ctx.userdata::<MyState>()
        let cmd_fn = Function::new(/* */).unwrap(); 

        ctx.globals().set("cmd", cmd_fn).unwrap();
    })
    .build();
```

See [examples/plugin.rs](examples/plugin.rs) for a complete example.

### Logger

All console output flows through a `Logger`. By default, `console.log` writes to stdout and `console.warn`/`console.error` write to stderr. Use `.log()` on the builder to provide a custom handler that receives a `LogLevel` and message:

```rs
use qjsrt::LogLevel;

let engine = JsEngine::builder()
    .log(|level, msg| match level {
        LogLevel::Debug => { /* silenced */ }
        LogLevel::Log => println!("{msg}"),
        LogLevel::Warn => eprintln!("[WARN] {msg}"),
        LogLevel::Error => eprintln!("[ERROR] {msg}"),
    })
    .build();
```

### Evaluation methods

`JsEngine` provides three ways to evaluate code:

- **`eval(code).await`** — evaluates as an ES module (supports `import`/`export`). Waits for all async work to complete. No return value.
- **`eval_script(code).await`** — evaluates as a script. Waits for async work, then returns the stringified last expression as `Result<String, String>`.
- **`eval_bytecode(bytes).await`** — loads and evaluates precompiled bytecode (from `compile()`). Waits for all async work to complete.
- **`eval_bytecode_detached(bytes)`** — same as `eval_bytecode` but returns immediately with a `oneshot::Receiver<()>`.
- **`eval_detached(code)`** — same as `eval` but returns immediately with a `oneshot::Receiver<()>` that signals completion. Useful when you want to keep doing work on the calling thread.

```rs
// blocking eval
engine.eval(r#"console.log("hello")"#).await;

// get a result back
let result = engine.eval_script("1 + 2").await; // Ok("3")

// run precompiled bytecode
let bytes = std::fs::read("app.bin").unwrap();
engine.eval_bytecode(bytes).await;

// non-blocking — poll or await the receiver
let done_rx = engine.eval_detached(r#"console.log("background")"#);
```

## CLI usage

```
qjsrt [file.js]              # run a JS file, or read from stdin
qjsrt -c [file.js] [-o out]  # compile to bytecode, -o required for stdin
qjsrt -b <file.bin>          # run a compiled binary
qjsrt -p '<expr>'            # evaluate and print
qjsrt -e '<expr>'            # evaluate silently
```

When no input file is given, `qjsrt` and `qjsrt -c` read source from stdin. The `-o` flag specifies the output path for `-c` (required when compiling from stdin).

Files are evaluated as ES modules with `import`/`export` support. Expressions (`-e`/`-p`) are evaluated as scripts.

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
cargo build
cargo test
```

Requires Rust 2021 edition. Licensed under MIT.
