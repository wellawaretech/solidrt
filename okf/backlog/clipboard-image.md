---
title: Copy and paste an image through the clipboard
description: "navigator.clipboard is text only; wanted an image write and read (image/png) on desktop through SDL_SetClipboardData and SDL_GetClipboardData, in the web shape with ClipboardItem over a Uint8Array, listed as \"clipboard-image\" in Flux.capabilities only where it works; the clipboard moves from alloy to ore with it; Android is out, since copying an image is not a normal Android action."
created: 2026-11-03
---

# Copy and paste an image through the clipboard

`navigator.clipboard` ([clipboard.rs](../../flux/src/standards_plugins/clipboard.rs))
supports only `readText` and `writeText`. An app can render a node to
pixels (`captureSnapshot`) and encode a PNG (`encodeImage`), but it cannot
put that image on the clipboard to paste into another app, and it cannot
take an image another app copied.

## What done looks like

An app writes a PNG to the clipboard and reads one from it on the
platforms that support it, and can see in advance where they do not.

The shape is the web's, with one documented simplification: flux has no
`Blob`, so a `ClipboardItem` holds a `Uint8Array`, as `navigator.share`
takes flux:fs handles for want of a `File`
([share.d.ts](../../packages/flux-types/standards/share.d.ts)).

```ts
await navigator.clipboard.write([new ClipboardItem({ "image/png": bytes })])

let items = await navigator.clipboard.read()
if (items.length > 0) {
  let image = decodeImage(await items[0].getType("image/png"))
}
```

- `write` takes exactly one item with the one type `image/png`. Anything
  else (no item, several, another type, a value that is not a
  `Uint8Array`) throws a `TypeError`. The write replaces what the
  clipboard held, text included: `readText` resolves `""` after it.
- `read` resolves to one item when the clipboard holds an image, and to an
  empty list when it does not. `item.types` lists `image/png`;
  `item.getType("image/png")` resolves to the PNG bytes, ready for
  `decodeImage`. Text stays with `readText`: `read` does not report
  `text/plain`, so there is one way to get each kind.
- `"clipboard-image"` is in `Flux.capabilities` only where an image write
  and read work. `write` and `read` elsewhere reject with
  `NotSupportedError`, as in
  [flux:dialog](../../packages/flux-types/gui/dialog.d.ts). The one
  capability covers both directions.
- Desktop (Linux, macOS, Windows) lists it.
- Android does not. Copying an image is not a normal Android action, and
  SDL's Android clipboard is text only. An app on Android shares an image
  instead (see [share-sheet](../done/share-sheet.md)).
- `Flux.capabilities` is the only place to check support. There is no
  `ClipboardItem.supports()`: a second channel for the same answer could
  drift from the first. A headless flux has no `navigator.clipboard` at
  all, as today.
- The contract in
  [clipboard.d.ts](../../packages/flux-types/standards/clipboard.d.ts) and
  the "text only" lines in `docs/runtime/index.md`,
  `docs/guides/coming-from-electron.md` and
  `docs/guides/coming-from-react-native.md` say what the clipboard now
  carries.

## What it involves

- The clipboard moves to ore. It is an OS facility
  ([platform-layering](../design/platform-layering.md)), new SDL work
  lands in ore, and this feature touches the clipboard, so the text
  clipboard moves with it: alloy's `SetClipboardText` /
  `GetClipboardText` commands (`alloy/src/event.rs`, `alloy/src/app.rs`)
  go, and an ore clipboard module carries text and image through the
  main-thread bridge (`ore/src/main_thread.rs`). The plugin stays
  `standards_plugins/clipboard.rs`, now over ore. Nothing new goes into
  `alloy/src/sdl_utils.rs`.
- The sdl3 crate (0.20.0) wraps only the text and primary-selection
  calls. `SDL_SetClipboardData`, `SDL_GetClipboardData` and
  `SDL_HasClipboardData` are in `sdl3::sys`; ore wraps them in one thin
  `unsafe` module. `SDL_SetClipboardData` takes MIME-typed data through a
  callback plus a cleanup callback, so ore owns the bytes until SDL
  releases them: the PNG stays in memory until the clipboard is replaced.
- SDL 3.4.18 covers the desktop targets: X11 and Wayland carry any MIME
  type, macOS maps `image/png` to its UTI, Windows writes it as the
  registered "PNG" format (see the open question).
- On Linux the app serves the clipboard's data, so a copied image is gone
  when the app exits unless a clipboard manager holds it. Text behaves
  the same today; the contract says so.
- The capability is a fact of the platform, added as `share` is
  (`flux/src/plugins/mod.rs`): an `available()` in the ore module decides.
- Under `sol test` the clipboard is the engine's own, never the OS's: the
  test engine lists the capability whatever its platform, a write is
  recorded with its bytes and MIME type for the test to assert, and a
  test puts an image on it for the app to read. ore keeps a per-engine
  session for this, the shape of `share::Session`. The dev client records
  too, and the control API and the MCP bridge read it back, as they do
  shares (`requestedShares`). Text goes through the same session.

## Open questions

- Windows and `CF_DIB`. SDL writes `image/png` as the registered "PNG"
  format only, and reads `image/png` only from it. A Windows app that
  reads or writes only `CF_DIB` (a plain bitmap) then pastes nothing from
  us and gives us nothing. SDL also carries `image/bmp` as `CF_DIB`, in
  the same call. Check which common apps need it (an image editor, an
  office suite, the screenshot key); if they do, ore offers both formats
  on a write and falls back to the bitmap on a read, converted to PNG, so
  the JS contract stays `image/png`.
