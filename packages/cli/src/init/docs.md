# srt init

{{ usage init }}

Scaffolds a new SolidRT project into a new (empty) folder: package.json,
tsconfig.json, AGENTS.md, a starter src/index.tsx, an empty assets/
(everything in it ships with the app) and an `.mcp.json`, then installs the
dependencies. The picker offers extensions on an interactive terminal.

The public entry point is `bun create solidrt <dir>`, which forwards here and
needs nothing installed first.
