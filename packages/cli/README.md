# @solidrt/cli

Developer tooling for SolidRT: `srt`, the command that runs, checks, bundles
and packs a `@solidrt/core` application.

> LLM agents: see [AGENTS.md](./AGENTS.md) for a dense, self-contained quickstart.

Bun is a dev prerequisite only; apps run on the bundled `flux` runtime.
Invoke via `bunx srt <command>` (a scaffolded project's scripts do).

## Commands

```sh
bunx srt init <dir>      # scaffold a new project into a new (empty) folder
bunx srt run [file]      # dev server + local client window
bunx srt server [file]   # dev server only
bunx srt client          # client only, attached to the project's dev server
bunx srt check [file]    # build and typecheck, writing nothing
bunx srt bundle [file]   # transpile to JS or bytecode (dist/bundle/)
bunx srt render [file]   # render frames offscreen, optionally replaying a script
bunx srt pack [file]     # bundle + compile to a standalone executable (experimental)
bunx srt mcp             # MCP server (stdio) exposing the running dev server to agents
```

`srt --help` lists every command and option; `srt --version` prints the
version. Run from the project root to work on the project (its entry is
`solidrt.entry` in package.json, default `src/index.tsx`); pass a file to
work on that file on its own.

The dev server does not watch for changes: push edits to connected clients
with the MCP `reload` tool (see [agents/debugging.md](./agents/debugging.md)),
or restart `srt run`.

The prose lives in [docs/](./docs/index.md).
