# Player

This is the client's built-in home screen: it lists installed apps, connects
to dev servers, and hosts launched apps. It is a regular SolidRT app; read
`../../packages/cli/scaffold/AGENTS.md` first, everything there applies.

Differences because this app lives in the solidrt monorepo:

- Dependencies are `workspace:*`; `bun install` at the repo root wires them.
  There is nothing to install here.
- `srt` is the in-repo CLI; run it as `bunx srt`.
- The production bundles are generated into `../../lattice/resources/`
  (committed there), because lattice embeds them into the client binary at
  compile time via `include_str!`. Rebuild them with `make player-bundle`
  from the repo root after changing any source here.
- `src/bsod.tsx` is a second entry point, bundled separately: the error
  screen the client shows when an app fails to start.
