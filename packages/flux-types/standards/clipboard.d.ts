// navigator.clipboard: the clipboard, text and a PNG image. Solidrt
// semantics on the standard shape: no permissions model and no clipboard
// events (a single known app does not ask itself for permission), a
// ClipboardItem holds Uint8Arrays (flux has no Blob), and an image is one
// item with the one type "image/png". Every call returns a promise, and
// every failure rejects; only the ClipboardItem constructor throws.
//
// GUI builds only (the clipboard belongs to the windowing platform): a
// headless flux has `navigator` (navigator.d.ts) but no
// `navigator.clipboard` and no `ClipboardItem`, so `"clipboard" in
// navigator` is the check for text. An image works only where
// "clipboard-image" is in `Flux.capabilities` (Linux, macOS and Windows;
// not Android); that is the one check for it, there is no
// `ClipboardItem.supports()`.
//
// On Linux the app serves what it copied: a copy, text included, is gone
// when the app exits unless a clipboard manager took it over. On macOS an
// image is served by the app too, when another app pastes, so an image
// copy there is likely gone when the app exits; text is written at once
// and stays.
//
// Under `sol test` the clipboard is the engine's own, never the OS's, and
// "clipboard-image" is listed whatever the platform: a test reads what the
// app wrote (`app.clipboard()`) and puts what the app pastes
// (`app.setClipboard()`). A dev client writes the OS clipboard and keeps
// the app's last write for an agent to read back.
//
// ```ts
// let bytes = encodeImage(await captureSnapshot(boardNode))
// await navigator.clipboard.write([new ClipboardItem({ "image/png": bytes })])
//
// let items = await navigator.clipboard.read()
// if (items.length > 0) {
//   let image = decodeImage(await items[0].getType("image/png"))
// }
// ```

/**
 * Typed bytes for the clipboard: a MIME type to a `Uint8Array` each. The
 * web's shape over `Uint8Array` in place of `Blob`.
 */
declare class ClipboardItem {
  /**
   * Copies the bytes of `record` (a snapshot: a later change to an array
   * does not reach the item). Any type string is taken here; `write` takes
   * only an item with the one type "image/png". Throws a TypeError for a
   * non-object, an empty record, a type that is not valid text (a lone
   * surrogate), or a value that is not a `Uint8Array`.
   */
  constructor(record: Record<string, Uint8Array>)
  /** The item's types, in the record's order (a frozen array). */
  readonly types: readonly string[]
  /**
   * Resolves to a fresh copy of the bytes of `type`. Rejects with a
   * TypeError for a type that is not a string, and with a
   * "NotFoundError" for a type the item does not hold.
   */
  getType(type: string): Promise<Uint8Array>
}

interface Clipboard {
  /**
   * Resolves to the clipboard's text, "" when the clipboard is empty or
   * holds no text (an image, after `write`). Rejects on platform failure.
   */
  readText(): Promise<string>
  /**
   * Replaces the clipboard's content with `text`; an empty text clears
   * it. Rejects with a TypeError for a non-string; rejects for a text
   * with a NUL character, under `sol test` too; and rejects on platform
   * failure.
   */
  writeText(text: string): Promise<void>
  /**
   * Resolves to the clipboard's image: one item with the one type
   * "image/png" when the clipboard holds an image, an empty list when it
   * does not. Text is never reported here (`readText` reads it). On
   * Windows a bitmap (what the PrintScreen key copies) is read as PNG too.
   *
   * Rejects with a "NotSupportedError" where "clipboard-image" is not in
   * `Flux.capabilities`, and on platform failure.
   */
  read(): Promise<ClipboardItem[]>
  /**
   * Replaces the clipboard's content, text included, with a PNG image:
   * `readText` resolves "" after it. `items` is exactly one item with the
   * one type "image/png", its bytes a PNG file (`encodeImage` writes
   * one). The bytes stay in memory until the clipboard is next replaced.
   *
   * Rejects with a TypeError for no item, several, an item with another
   * type, or bytes that are not a PNG; then with a "NotSupportedError"
   * where "clipboard-image" is not in `Flux.capabilities`; and on platform
   * failure.
   */
  write(items: ClipboardItem[]): Promise<void>
}

interface Navigator {
  readonly clipboard: Clipboard
}
