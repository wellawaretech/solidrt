---
title: Copy an image to the clipboard
description: "navigator.clipboard is text only; wanted an image write (image/png) on desktop through SDL_SetClipboardData, listed as \"clipboard-image\" in Flux.capabilities only where it works; Android is out, since copying an image is not a normal Android action."
created: 2026-11-03
---

# Copy an image to the clipboard

`navigator.clipboard` ([clipboard.rs](../../flux/src/standards_plugins/clipboard.rs))
supports only `readText` and `writeText`. An app can render a node to
pixels (`captureSnapshot`) and encode a PNG (`encodeImage`), but it cannot
put that image on the clipboard to paste into another app.

## What done looks like

An app writes a PNG to the clipboard on the platforms that support it, and
can see in advance where they do not.

- `"clipboard-image"` is in `Flux.capabilities` only where an image write
  works. A write elsewhere rejects with `NotSupportedError`, as in
  [flux:dialog](../../packages/flux-types/gui/dialog.d.ts).
- Desktop (Linux, macOS, Windows) lists it.
- Android does not. Copying an image is not a normal Android action, and
  SDL's Android clipboard is text only. An app on Android shares an image
  instead (see [share-sheet](share-sheet.md)).
- `Flux.capabilities` is the only place to check support. There is no
  `ClipboardItem.supports()`: a second channel for the same answer could
  drift from the first.

## What it involves

- SDL3's `SDL_SetClipboardData` takes MIME-typed data through a callback.
  If the sdl3 crate does not wrap it, add a wrapper in
  `alloy/src/sdl_utils.rs`.
- Under `sol test`, the test engine lists the capability and records the
  write, so a test can assert the bytes and the MIME type.

## Open questions

- The shape. The web shape is
  `navigator.clipboard.write([new ClipboardItem({ "image/png": blob })])`,
  but flux has no `Blob`. Either add a minimal `Blob` (or let
  `ClipboardItem` take a `Uint8Array`, a documented simplification), or
  use a smaller call outside the web shape.
- Reading an image back (`navigator.clipboard.read()`): needed now, or
  only the write.
