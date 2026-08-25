# srt check

{{ usage check }}

The gate worth wiring into CI: it builds and typechecks the app without
producing build output. With a folder it covers every entry under it
(`src/index.tsx`, `examples/*`, and in a monorepo every example app and
package example), so one call at the repo root answers "did I break any
example" before pushing. The dev server runs the same check once at
startup, without gating on it.
