# @solidrt/cli - agent notes

Dense, self-contained facts for running and verifying a SolidRT app. Full docs
live in docs/ (and the website). For the authoring model (elements, props,
reactivity), see @solidrt/core (its AGENTS.md).

`srt` is the dev tool. Bun is a dev prerequisite only; SolidRT apps run on the
bundled `flux` runtime, not on Bun. Invoke via `bunx srt <command>`.

## Commands

- `bunx srt init <dir>` - scaffold a new SolidRT project into a new (empty)
  folder: package.json, tsconfig.json, AGENTS.md, a starter src/index.tsx, then
  installs deps. Greenfield shortcut (no install needed first):
  `bun create solidrt <dir>`.
- `bunx srt run src/index.tsx` - dev server + a local client window, watches and
  hot-reloads. NEEDS A DISPLAY (opens a GUI window). Not usable headless.
- `bunx srt bundle src/index.tsx` - transpile to `<file>.srt.js`. With
  `--compile`, emits `.srt.bin` bytecode. `--minify`, `--dev`, `--stdout`,
  `--output` also available.
- `bunx srt record src/index.tsx [flags]` - run live and record input events
  (keydown/keyup so far) to a script file, for replaying with `render`.
- `bunx srt render src/index.tsx [flags]` - render OFFSCREEN to PNG frames,
  optionally replaying a `--script` file recorded with `record`.
- `bunx srt server [file]` / `bunx srt client` - the two halves of `run`
  separately (server distributes code; clients on other devices connect to it).

## Verifying without a display (headless / CI / agent box)

Two reliable checks that need no GUI:

1. `bunx srt bundle src/index.tsx` - exit 0 means the app compiles. Fast.
2. `bunx srt render src/index.tsx --size 480x640 --duration 1 --fps 2` -
   renders offscreen via EGL/wgpu and writes `frame-NNNNNN.png`. This actually
   proves the app renders. Combine with `--fps`/`--duration` (defaults
   1280x720, 60fps, 1s).

`render` gotchas:
- Frames are written to the RUNTIME's working dir (`~/.local/share/SolidRT/go/`),
  NOT the directory you ran the command from. Look there for the PNGs.
- The recording includes a debug overlay (FPS/REQ/MiB/CPU) in a corner.
- Run from the project directory. There is no `bunx --cwd` flag.

## Dev server proxies (when clients on other devices need your machine's data)

- `--proxy-http` - route `fetch` through the dev server; responses cached in
  `.srt-cache.db` (delete the file to clear).
- `--proxy-files` - route flux:fs (`file`/`dir`/`write`) through the dev server.
  Exposes your dev machine's files to all clients; use with care.

## REPL (opened by `run`/`server`)

`load <file>`, `reload [n]`, `stop [n]`, `list`, `!<cmd>`, `quit`/`exit`.
