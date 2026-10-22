# sol init

{{ usage init }}

Scaffolds a new SolidRT project into a new (empty) folder: package.json,
tsconfig.json, README.md, AGENTS.md, a starter src/ (entry src/index.tsx), a starter test
(tests/app.test.tsx, run with `bun run test`), an empty assets/ (everything in
it ships with the app) and an `.mcp.json`, then installs the
dependencies. The picker offers extensions on an interactive terminal.

`--runtime` adds the app's own runtime: `runtime/`, a cargo project over
lattice with a starter module (`runtime:hello`, its types in
`runtime/types/`, a test in `tests/`), named in package.json as the app
runtime and built with `make -C runtime`. With no folder given, it adds the
runtime to the project in the current directory. The runtime's player is
named after the app, so it asks for an Android application id (`solidrt.appId`,
reverse-DNS) when package.json has none; without a terminal, set it first.
See the runtime docs, "A custom runtime".

The public entry point is `bun create solidrt <dir>`, which forwards here and
needs nothing installed first.
