# qjsrt

An embeddable, extensible cross-platform JavaScript runtime in Rust built on [QuickJS-NG](https://github.com/quickjs-ng/quickjs) with a Tokio-based async event loop.

## Usage

```rs
use qjsrt::run;

fn main() {
    let code = r#"
        print('hello, world!')
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
    })
    .build();
```

See [examples/plugin.rs](examples/plugin.rs) for a complete example.

### Evaluation methods

`JsEngine` provides three ways to evaluate code:

- **`eval(code).await`** — evaluates as an ES module (supports `import`/`export`). Waits for all async work to complete. No return value.
- **`eval_script(code).await`** — evaluates as a script. Waits for async work, then returns the stringified last expression as `Result<String, String>`.
- **`eval_detached(code)`** — same as `eval` but returns immediately with a `oneshot::Receiver<()>` that signals completion. Useful when you want to keep doing work on the calling thread.

```rs
// blocking eval
engine.eval(r#"print("hello")"#).await;

// get a result back
let result = engine.eval_script("1 + 2").await; // Ok("3")

// non-blocking — poll or await the receiver
let done_rx = engine.eval_detached(r#"print("background")"#);
```

## CLI usage

```
qjsrt <file.js>            # run a JS file (ES module)
qjsrt -p '<expr>'          # evaluate and print
qjsrt -e '<expr>'          # evaluate silently
```

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

### Other 

- `print(msg)`    // print to stdout

## Building

```
cargo build
cargo test
```

Requires Rust 2021 edition. Licensed under MIT.
