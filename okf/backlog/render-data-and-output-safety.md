---
title: Keep sol render off the dev client's data and the user's files
description: sol render runs in the dev client's own data tree (client 0) even while that client is up, so a render reads and writes the live app's saved state; it wipes dist/render/ for its staging, taking any frames a user wrote there with -o; and a missing -o directory fails only after every frame has rendered.
created: 2026-10-08
---

# Keep sol render off the dev client's data and the user's files

## 1. Render shares the live client's data tree

By design a render runs in the same sandbox as a dev client
(`clientStorageArgs`: `--data-root` and `--client`, default 0), so the
frames show the state a dev session built (the render command's docs.md).
With a dev client up on that tree, the runtime warns ("another client is
already using ...; two clients on one tree corrupt each other's data",
`claim_run_marker` in [storage.rs](../../lattice/src/storage.rs)) and
carries on. The render then reads and writes the live app's data: an
autosave on mount lands in the dev client's draft. A render is a throwaway
run, and its writes should never reach a tree it does not own, whether or
not a client is up at the time.

**Done when:** a render's writes never land in a client tree, the frames
still show the dev state by default (the documented contract), and two
renders and a dev client can run at once.

**Involves:** render from a copy. The CLI copies the client tree into a
scratch tree under the render's staging dir (wiped with the rest of it) and
points the runtime there. To settle: how much gets copied (a fetch disk
cache can be large and could stay behind), and whether `--client N` keeps
meaning "start from client N's state". Refusing to render while the tree is
held does not solve it: a render run with no client up still writes into
the dev tree.

## 2. The staging wipe takes the user's output

Render stages its bundle, isolates and assets in `dist/render/` and wipes
that directory first ([render/main.ts](../../packages/cli/src/render/main.ts);
each flow wipes only its own subdir, okf/done/build-output-dirs.md). With
`-o dist/render/phone` the frames land inside that subdir, so the next render
deletes them. `sol bundle -o` already refuses a directory it does not own.

**Done when:** render refuses an `-o` inside its staging dir with a message
naming the dir, before anything is wiped.

## 3. A missing output directory fails after the whole render

The PNG writer runs on its own thread, and the driver only joins it after
the last frame (`drive` in [render_host.rs](../../lattice/src/render_host.rs)).
So `-o dist/frames/f` with no `dist/frames/` renders every frame and then
fails on the first write ("could not write ...f-000000.png").

**Done when:** the CLI creates the prefix's directory before it launches the
runtime (an output path is a request to write there), and the runtime checks
the directory before the first frame and stops at the first failed write,
for callers that pass `--out` directly.
