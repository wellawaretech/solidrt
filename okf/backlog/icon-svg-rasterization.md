---
title: Rasterize icon SVGs with the runtime's own renderer
description: The Android launcher icon needs a PNG the TypeScript CLI cannot produce from icon.svg, so a derived icon.png is checked in and every app must maintain one by hand; the runtime can rasterize it itself via captureSnapshot + encodeImage.
created: 2026-09-01
---

# Rasterize icon SVGs with the runtime's own renderer

`srt pack --apk` needs a raster PNG for the launcher icon, but the pack
pipeline is pure TypeScript and cannot rasterize `icon.svg`: `flux:svg`
parses to draw data and `flux:image` encodes RGBA8, with no CPU path
rasterizer between them - rasterization lives in the alloy stack. So today
a pre-rendered `icon.png` sits beside `icon.svg` (in the scaffold it is
checked in, rendered once out-of-band with rsvg-convert), and keeping the
pair in sync is a manual job for every app.

The runtime already has everything needed to close the loop in-house, no
external rasterizer:

1. render the `<svg>` element at the target size (full usvg fidelity, the
   same renderer the launcher tiles use),
2. `captureSnapshot(nodeId)` (`flux:gpu`) - premultiplied RGBA8,
3. `encodeImage(img)` (`flux:image`) - PNG bytes, premultiplied-to-straight
   handled by default,
4. write the file with `flux:file`.

Steps 1-2 need the GUI runtime (a window/GPU context), which is why this is
"run a small solidrt app once", not a headless flux script - fine on a dev
machine, a consideration for CI.

Two landing shapes, additive:

- **Repo script** (minimal first stage): `scripts/render-icon.tsx` renders
  a given SVG to a PNG sibling; its first job is keeping the scaffold logo
  pair (`packages/cli/src/init/scaffold/icon.{svg,png}`) in sync.
- **Pack time**: `srt pack --apk` rasterizes `assets/icon.svg` itself, and
  the PNG convention, the checked-in scaffold `icon.png`, and the
  "add assets/icon.png" note in `resolveLauncherIcon`
  (`packages/cli/src/pack/main.ts`) all disappear. This needs the CLI to
  boot a GUI context mid-pack (or shell out to the runner binary it
  already resolves), so it is deliberately not the first stage.

## Related

- `app-icons.md` - the icon pipeline this feeds.
- `standalone-android-apk.md` - notes the gap ("the TypeScript CLI cannot
  rasterize SVG"); this item is its resolution path.
