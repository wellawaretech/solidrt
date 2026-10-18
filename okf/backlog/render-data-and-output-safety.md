---
title: Keep sol render off the dev client's data
description: sol render runs in the dev client's own data tree (client 0) even while that client is up, so a render reads and writes the live app's saved state.
created: 2026-10-08
---

# Keep sol render off the dev client's data

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
the dev tree. Two renders at once also share the one staging dir
(`.solidrt-data/render/`), which each wipes at its start, so the staging
dir becomes one per render too.

The other half this item had, the staging wipe taking frames written into
it with `-o`, closed with the output split of 2026-10-18
(okf/done/build-output-dirs.md): staging moved to `.solidrt-data/render/`,
frames to `renders/`, and an `-o` inside the staging dir is refused.
