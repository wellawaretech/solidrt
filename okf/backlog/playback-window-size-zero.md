---
title: windowSize() reads 0x0 in headless playback
description: Playback synthesises a Resize for layout but never delivers it to JS, so any app sizing off windowSize() renders as though the window were empty - quietly, with a wrong-looking capture rather than an error.
created: 2026-08-06
---

# windowSize() reads 0x0 in headless playback


Playback synthesises one `Resize` event so layout gets its size, but the
JS-side window state never sees it: init events reach JS by an `AlloyCommand`
that only the interactive loop handles. So in any headless render
`windowSize()` is `{ width: 0, height: 0 }` (`displayScale()` reads 1, which
happens to be right now that playback pins the scale).

The failure mode is quiet. An app that sizes anything off `windowSize()` -
a responsive layout, a full-bleed background, a `pct()` fallback computed in
JS - renders as though the window were empty, and the capture looks wrong
rather than erroring. `changelog-shot` sidesteps it by laying out from the
content and capturing the node's own box, which is why it never noticed.

Fix is to deliver playback's synthesised resize to JS the same way the
interactive loop does, so `windowSize()` reports the `--size` the caller
asked for. Recorded in the closed determinism item, moved here so it is
visible in the open list.

Split out of a two-item "headless render loose ends" file when okf was
restructured; the other half is
[playback-shutdown-sigabrt](playback-shutdown-sigabrt.md). Both surfaced while
building `scripts/changelog/` on the now-headless `srt render`, and neither
blocks it.
