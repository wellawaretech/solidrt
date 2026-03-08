# qjsrt

A cross-platform JavaScript runtime in Rust built on [QuickJS-NG](https://github.com/quickjs-ng/quickjs) with a Tokio-based async event loop.

## CLI usage

```
qjsrt <file.js>            # run a JS file (ES module)
qjsrt -p '<expr>'          # evaluate and print
qjsrt -e '<expr>'          # evaluate silently
```

Files are evaluated as ES modules with `import`/`export` support. Expressions (`-e`/`-p`) are evaluated as scripts.

## API

### Timers

```js
const id = setTimeout(cb, ms);
clearTimeout(id);

const id = setInterval(cb, ms);
clearInterval(id);
```

### I/O

`io.source(target)` creates a source object from a file path or HTTP URL.

```js
const src = io.source("data.json");   // file
const src = io.source("https://api.example.com/data");  // HTTP GET
```

The source object has three body methods, each returning a Promise. The body can only be consumed once (web `Response`-style):

```js
const text = await src.text();    // string
const bytes = await src.bytes();  // Uint8Array
const obj = await src.json();     // parsed JSON
```

The source also exposes its target as a `path` (file) or `url` (HTTP) property.

### Other globals

- `print(msg)` — print to stdout

## Building

```
cargo build
cargo test
```

Requires Rust 2021 edition. Licensed under MIT.
