# qjsrt

A small JavaScript runtime built on [QuickJS-NG](https://github.com/aspect-build/rules_quickjs) (via [rquickjs](https://github.com/aspect-build/rules_quickjs)) with a Tokio-based async event loop.

## Usage

```
qjsrt <file.js>            # run a JS file (ES module)
qjsrt -p '<expr>'          # evaluate and print
qjsrt -e '<expr>'          # evaluate silently
```

Files are evaluated as ES modules with `import`/`export` support. Expressions (`-e`/`-p`) are evaluated as global scripts and return the result of the last expression.

The process stays alive until all timers (`setTimeout`, `setInterval`) have completed or been cleared.

## Available globals

- `print(msg)`
- `setTimeout(cb, ms)` / `clearTimeout(id)`
- `setInterval(cb, ms)` / `clearInterval(id)`

## Building

```
cargo build
cargo test
```

Requires Rust 2021 edition.
