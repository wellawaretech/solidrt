# flux

An embeddable, extensible cross-platform JavaScript runtime in Rust built on [QuickJS-NG](https://github.com/quickjs-ng/quickjs) with a Tokio-based async event loop.

## Usage

```rs
use flux::FluxEngine;

let bytecode = std::fs::read("app.bin").expect("read bytecode");
FluxEngine::new().eval(bytecode).await;
```

### Builder options

```rs
FluxEngine::builder()
    .logger(|level, msg| eprintln!("[{level:?}] {msg}"))
    .plugin(|ctx| { /* register extra globals */ })
    .userdata(my_value)          // store Rust data accessible inside plugins
    .module_override("flux:fs", MyFsModule)
    .stack_size(2 * 1024 * 1024)
    .build()
```

### Evaluation

All evaluation runs as ES modules.

- `eval(bytecode).await` - run precompiled bytecode
- `eval_source(code).await` - run JS source directly (requires `compile` feature)

## Compiling to bytecode

Build `fluxc` with `make fluxc`, then pipe JS source to it:

```
echo 'console.log("hello")' | fluxc > app.bin
```

`compile_source(source, module_name)` is also available as a library function.

## JavaScript API

### Web-standard globals

| API                                | Notes                                                         |
| ---------------------------------- | ------------------------------------------------------------- |
| `console.log/warn/error`           | routed through the configured logger                          |
| `setTimeout` / `clearTimeout`      |                                                               |
| `setInterval` / `clearInterval`    |                                                               |
| `performance.now()`                | ms since process start; host-overridable via `Clock` userdata |
| `fetch(url, opts?)`                | returns a `Response`                                          |
| `Request` / `Response` / `Headers` | web-standard                                                  |
| `TextEncoder` / `TextDecoder`      |                                                               |
| `crypto.subtle.digest(alg, data)`  | SHA-256/384/512 only; resolves to an `ArrayBuffer`            |

### `Flux` global

`Flux.on(event, handler)` - subscribe to events emitted by the host.

### `flux:http`

```js
import { serve } from "flux:http"

let server = serve({
  port: 3000, // 0 or omitted: the OS picks one, read it back from server.port
  fetch(req) {
    return new Response("ok")
  },
  routes: {
    "/health": new Response("ok"),
    "GET /users/:id": (req) => new Response(req.params.id),
  },
  error(err) {
    return new Response("error", { status: 500 })
  },
})

server.stop()
```

### `flux:fs`

```js
import { file, dir } from "flux:fs"

let text = await file("data.txt").text()
let bytes = await file("img.png").bytes()
await file("out.txt").write("hello")
await file("out.txt").remove()
let stat = await file("data.txt").stat()

let entries = await dir("./src").list()
```

### `flux:sqlite`

```js
import { Database } from "flux:sqlite"

let db = await Database.connect("app.db")
let stmt = db.query("SELECT * FROM users WHERE id = ?")
let rows = await stmt.all(42)
let row = await stmt.first(42)
await stmt.run(42)
db.close()
```

## Building

Run from `flux/`:

```
make build    # flux + fluxc + fluxrt binaries (release)
make test     # run tests
make clean
```

Use `PROFILE=debug` for debug builds. `make build-opt` produces a stripped, LTO-optimised binary.
