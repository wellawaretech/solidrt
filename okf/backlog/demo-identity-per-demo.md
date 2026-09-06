---
title: A demo shows up in the player under its project's name, not its own
description: srt demo serves demos/ as one project, so the player entry, appId and storage are the project's; the gallery appeared as "SolidRT components demos". Renamed for now (one demo per package); a second demo in a package needs per-demo identity derived from the slug.
created: 2026-08-26
---

# A demo shows up in the player under its project's name

What it looks like: run `srt demo components/gallery` and the player lists
the app as the demos project's `displayName` ("SolidRT components demos" at
the time), because `srt demo` serves `demos/` as one project and every bit
of identity - appId, displayName, icon, storage dir - is per project
(`lib/project.ts`). Nothing per demo reaches the player.

Interim, done 2026-08-26: the two demos projects carry the demo's name as
their `displayName` ("Gallery", "The Third Dimension"). Correct exactly as
long as each package ships one demo.

## What done looks like

A second demo in the same `demos/` folder gets its own player entry and
its own storage, without the demos README growing a per-demo metadata
section: derive the identity from the demo slug in `src/demo/main.ts`
(`the-third-dimension` -> "The Third Dimension", appId
`<project appId>.<slug>`) and hand it to the server as an override of the
project's fields. Roughly: a `--display-name`/`--app-id` pair on the server
args that `launchServer` passes only for `demo`, applied over
`readProject()`'s result in one place so the rest of the pipeline still
sees an ordinary project.

See okf/done/package-demos.md for why demos are one project per package.
