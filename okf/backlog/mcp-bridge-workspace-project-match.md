---
title: MCP bridge - match dev servers in subdirectories of the bridge's project
description: The bridge resolves a dev server by exact projectDir equality, so a workspace-root bridge reports "No dev server" for an entry served from packages/*/examples/ (its nearest package.json makes THAT directory the projectDir). Accept servers whose projectDir sits under the bridge's project, preferring an exact match, so MCP tools work in a monorepo without new CLI surface.
created: 2026-08-24
---

# MCP bridge: workspace project match

## Symptom

`srt mcp` resolves "which dev server do I talk to" by projectDir: the
nearest package.json above the bridge's cwd, compared with `sameDir`
against each record in `~/.solidrt/servers/*/live.json` (mcp.ts, the
`matches` filter and the probe's authoritative re-check). `srt run`
derives a server's projectDir the same way from the ENTRY - so in this
repo, running `packages/3d/examples/pick.tsx` registers `packages/3d`
while the bridge at the repo root looks for the root. Result: "No dev
server" while one is plainly running, and every monorepo verification
session falls back to hand-rolled curl against the control API. A
scaffolded app (one package.json) never sees this.

## Shape

Bridge-side fix, no new CLI surface: treat a server whose projectDir is
the bridge's project OR a subdirectory of it as a match, preferring an
exact match when both exist. In a workspace the root bridge legitimately
speaks for the packages below it; the reverse (a bridge in packages/3d
matching a root server) stays out - the narrower cwd is a deliberate
narrowing.

Two sites, one rule:

- the `matches` filter over the live records;
- the probe's re-check of the server's self-reported projectDir (the
  record is a hint, the server stays authoritative).

The "more than one server" arbitration gains a step: exact matches win
over subdirectory matches before the count is judged, so a root server
plus a package server coexist without tripping the ambiguity error for
either bridge. Subdirectory matches with no exact match and more than
one candidate keep the existing "pass -s <N>" error, now with the
listing that already prints.

Path comparison must reuse `sameDir`'s normalization (the two sides come
from different processes); a prefix check on raw strings is the known
trap (trailing separators, symlinked tmp).

Alternative considered and rejected: an `srt run --project <dir>` flag
pinning the registered projectDir. It fixes the same mismatch but adds
user-visible surface for a workspace-only problem, and every future
session has to remember to pass it; the bridge-side match fixes it for
good.

## Done looks like

From the repo root with `packages/3d/examples/pick.tsx` served, the
repo-root bridge's `list_clients` finds the server without flags; a
second server started for a scaffolded app elsewhere is untouched; two
servers under one workspace still resolve (exact beats subdirectory,
ambiguity error otherwise). The bridge is long-lived: the fix only takes
effect in a re-spawned bridge, which the verification protocol already
notes.
