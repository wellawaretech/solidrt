---
title: Cap the text atlas below the device's texture limit
description: The text atlas grows by doubling up to the GPU's maximum texture side before it evicts anything, so a churning repertoire takes it to 16384x8192 on a desktop GPU, 128 MiB of R8 texture held for the rest of the process; browsers hold a few 2048 pages and evict at the page. Cap the side at a fixed, modest size and let the existing two-second eviction run there; go multi-page only if a real app overflows it.
created: 2026-11-19
---

# Cap the text atlas below the device's texture limit

Part of stage 5 of [own-renderer](own-renderer.md): that stage is not done until this item is.

## Symptom

`TextAtlas` (alloy/src/rendertree/text/glyphs/text_atlas.rs) starts its
packer at `PROVISIONAL_MAX_SIDE` (2048) and the first frame raises the
cap to the device's `max_texture_size` (composite.rs). `AtlasPacker::insert`
grows by doubling the smaller side until that cap, and evicts (cells
unused for `EVICT_AFTER_FRAMES`, two seconds) only once it cannot grow.
On a desktop GPU the cap is 16384, so a scratch app stepping font sizes
every frame took the atlas through 32, 64 and 128 MiB of R8 texture
without evicting a cell. The atlas never shrinks, so the texture stays at
its high-water mark for the process's life. Since
okf/done/text-after-direct-draw.md the CPU side holds only the live
cells' pixels, so the 128 MiB is now GPU memory alone, which is still a
lot for text: Skia keeps at most four 2048-square pages per format (16 MiB
of A8) and evicts least-recently-used plots when a page is full,
WebRender allocates 2048-square pages on demand and frees the ones that
empty, Impeller caps its atlas well below the device limit and rebuilds it
from the live set.

A normal app does not get there: its styles are few and its repertoire
small, and the warm-up makes printable ASCII at three phases per style,
which is a few hundred cells. The ceiling is wrong regardless; a terminal
or a CJK document at several sizes would find it.

## Done looks like

The text atlas side is capped at a fixed, modest size (4096 is 16 MiB of
R8 and holds several thousand cells at 16 px; a named constant, raised
only by measurement), the device limit only lowering it on a GPU that
cannot take that much. At the cap the existing eviction runs: what nobody
drew in two seconds goes, the rest repacks. A live set that does not fit
even then is an overflow, logged once as today, and the signal to go
multi-page (a second texture, a batch per page), which this item does not
do. The cap is a policy, so it is a `TextAtlas` constructor parameter or a
setter that lattice sets, not a number buried in alloy.

A smaller follow-on, or part of this if it is cheap: shrink back after a
churn. A repack that finds the live cells fit in half the side packs them
into the smaller texture; `Dirty::Whole { resized: true }` already carries
a size change to the upload path.

## Involves

- `text_atlas.rs`: the cap constant and `set_texture_cap` taking the
  minimum of the policy cap and the device limit; the `atlas.rs` header
  says growth stops at the cap and eviction runs there, which still holds.
- A test in alloy/src/tests/glyphs.rs: a packer capped at a small side
  under churn evicts and stays at the cap; the live set survives.
- A churn probe (a text stepping sizes every frame) read through `/stats`
  and the client's memory, before and after, on the desktop and the TV.
