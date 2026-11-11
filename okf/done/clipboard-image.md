---
title: Copy and paste an image through the clipboard
description: "Landed: navigator.clipboard.write and read carry a PNG image (one ClipboardItem over a Uint8Array, the one type image/png) on desktop through SDL_SetClipboardData and SDL_GetClipboardData, listed as \"clipboard-image\" at the gui seam; the clipboard, text included, moved from alloy to an ore session the test host and the control API read; a Windows bitmap reads as BMP from ore and is converted to PNG in the plugin; Android stays text only."
created: 2026-11-03
completed: 2026-11-11
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
  else (no item, several, another type, bytes that are not a PNG) rejects
  with a `TypeError`; the `ClipboardItem` constructor throws one for a
  value that is not a `Uint8Array`. The write replaces what the
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
  calls. `SDL_SetClipboardData` and `SDL_GetClipboardData` are in
  `sdl3::sys`; ore wraps them in one thin `unsafe` module (a read asks
  for the data directly, without `SDL_HasClipboardData`; see Decisions). `SDL_SetClipboardData` takes MIME-typed data through a
  callback plus a cleanup callback, so ore owns the bytes until SDL
  releases them: the PNG stays in memory until the clipboard is replaced.
- SDL 3.4.18 covers the desktop targets: X11 and Wayland carry any MIME
  type, macOS maps `image/png` to its UTI, Windows writes it as the
  registered "PNG" format (see the Windows bitmap split under Decisions).
- On Linux the app serves the clipboard's data, so a copied image is gone
  when the app exits unless a clipboard manager holds it. Text behaves
  the same today; the contract says so. On macOS SDL serves an image
  lazily too (a pasteboard data provider) but writes text at once "so
  text is available after the app quits"; the contract says an image
  copy there is likely gone at exit (not checked on a Mac).
- The capability is a fact of the platform: `image_available()` in the
  ore module decides, and it is listed at the gui seam (see Decisions).
- Under `sol test` the clipboard is the engine's own, never the OS's: the
  test engine lists the capability whatever its platform, a write is
  recorded with its bytes and MIME type for the test to assert, and a
  test puts an image on it for the app to read. ore keeps a per-engine
  session for this, the shape of `share::Session`. The dev client records
  too, and the control API and the MCP bridge read it back, as they do
  shares (`requestedShares`). Text goes through the same session.

## Decisions

- The capability is added at the gui seam (`register_capabilities` in
  `flux/src/alloy_plugins/mod.rs`), not with the base capabilities as
  `share` is: only the gui seam defines `navigator.clipboard`, so a
  headless desktop flux listing it would be false.
- The session holds one value, the clipboard's content, not a log as the
  link and share sessions do: a PNG is large, and only the last write is
  on the clipboard anyway. In a record-only session that value is the
  clipboard; in any other it is the app's last successful write.
- An agent reads the app's last write (`/__control__/clipboard`,
  `get_clipboard`), never the OS clipboard: another app may have replaced
  it, and reading it would hand an agent whatever the user copied. An
  agent cannot set the clipboard; a test can (`app.setClipboard`).
- Bad `write` arguments reject with a TypeError, as every failure of the
  module rejects; only the `ClipboardItem` constructor throws (a non-object,
  an empty record, a value that is not a `Uint8Array`). Reading the
  arguments of `write`, `writeText` and `getType` can run script (an
  index getter on `write`'s array) or fail to convert (a lone
  surrogate); each such error
  is taken off the context and rejects, and the length of `write`'s array
  is read as a number, never through `Array::len`, whose int32 assertion a
  script can trip. `write` checks the PNG signature, so bytes that are not
  a PNG never reach another app.
- An image read is one `SDL_GetClipboardData` call per format, not a
  `SDL_HasClipboardData` first: on X11 the check fetches the whole
  selection, and the owner can change between the two calls.
- After a failed OS write the session holds nothing: SDL drops the old
  data before it hands the new one to the platform, and a failed image
  write clears the clipboard.
- The text clipboard moved with the image: alloy's `SetClipboardText` /
  `GetClipboardText` commands are gone, and text goes through the same
  ore session and the main-thread bridge. It now works under `sol test`:
  the stepped loop used to drop the commands.
- The Windows bitmap split: ore passes the clipboard's bytes on with their
  format (`ImageFormat::Bmp` for a `CF_DIB`, which SDL hands over as a
  BMP file), and the flux plugin converts a BMP to PNG through forge's
  codec, so no codec is in ore and the JS contract stays `image/png`.

## Landed

- `ore/src/clipboard.rs` (the session, `image_available`, the formats) and
  `ore/src/clipboard/sdl.rs` (the one `unsafe` module over `sdl3::sys`;
  the ownership of the offered bytes is checked against SDL 3.4.18's
  `SDL_SetClipboardData` and `SDL_CancelClipboardData`). Unit tests in
  `ore/src/tests/clipboard.rs`.
- `flux/src/standards_plugins/clipboard.rs` over the session, with the
  `ClipboardItem` class; `flux::clipboard` exported for the embedder.
- lattice: the stepped engines set the session to record only; `sol:test`
  `clipboard()` / `setClipboard()`; the `clipboard` control query
  (`lattice/src/clipboard.rs` spells the JSON).
- `@solidrt/test` `app.clipboard()` / `app.setClipboard()`,
  `/__control__/clipboard`, the MCP tool `get_clipboard` (a PNG comes
  back as an image), the types in `packages/flux-types` and the docs.
- App tests in `packages/test/tests/readers.test.tsx`.
- Verified on Linux (Wayland) with the dev client: an image and a text
  the app writes reach `wl-paste` (`image/png`, the right size), and the
  app reads its own write back. A paste from another app (`wl-copy`) read
  as empty while the app's window had no keyboard focus: Wayland sends
  the selection only to the focused client, so that direction was not
  seen here.
- The Windows bitmap read fallback landed compiled but not run on
  Windows; macOS and Android are compiled by CI only. The rest is
  [clipboard-image-windows-bitmap](../backlog/clipboard-image-windows-bitmap.md).
