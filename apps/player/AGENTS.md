# Player

This is the client's built-in home screen: it lists installed apps, connects
to dev servers, and hosts launched apps. It is a regular SolidRT app; read
`../../packages/cli/scaffold/AGENTS.md` first, everything there applies.

Differences because this app lives in the solidrt monorepo:

- Dependencies are `workspace:*`; `bun install` at the repo root wires them.
  There is nothing to install here.
- `sol` is the in-repo CLI; run it as `bun run sol`.
- The production bundles are generated into `../../packages/cli/player/`
  (not tracked), because every runtime (the stock program crate in
  `runtime/`, a custom one) embeds them into its binaries at compile time
  via `include_str!`, and the CLI package ships them. Rebuild them with
  `make player-bundle` from the repo root after changing any source here.
- `src/bsod.tsx` is a second entry point, bundled separately: the error
  screen the client shows when an app fails to start.
