---
title: HiDPI image cursors from one SVG source
description: createCursor takes encoded bitmaps per scale; a vector source rasterized at 1x, 2x and 3x in JS (parseSvg, draw to a texture, readTexture) would give sharp cursors on every display from one asset, without a rasterizer in the production runtime.
created: 2026-09-22
---

# HiDPI image cursors from one SVG source

## Symptom

An app with an image cursor supplies a 1x bitmap and, if it wants a sharp
cursor on a 2x display, a second bitmap at exactly twice the size
(`createCursor({ image, alternates: { 2: bytes2x } })`). Two hand-made
assets per cursor, and a 1.5x or 3x display still gets a scaled one. The
window icon in the go client already does better: one SVG, rasterized to
size (lattice/src/go/icon.rs), but that path uses resvg, which the
production runtime deliberately does not ship.

## What done looks like

`createCursor({ svg, size: 32, hotspot })` (or an `svg` source on the
frame shape): the runtime rasterizes the document at 1x, 2x and 3x of
`size`, registers the 1x image with the others as alternates, and the
platform picks per display scale. One asset, sharp everywhere the
platform honors alternates (macOS, Wayland, Windows with the DPI hint).

## Shape

Entirely in JS over what exists: `parseSvg` (forge's usvg, geometry only)
plus a `<d-view>` draw of the document into a texture target at each
scale and `readTexture` back to straight-alpha RGBA8, fed to the same
`flux:rendertree` `createCursor` binding. The draw needs a frame, so the
helper is async or takes a texture the app rendered; decide which reads
better against `createImage`'s async precedent. No engine change.
