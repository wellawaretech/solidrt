// navigator.clipboard: the clipboard, text only. Solidrt semantics on the
// standard shape: no permissions model and no clipboard events - a single
// known app does not ask itself for permission. GUI builds only (the
// clipboard belongs to the windowing platform); a headless flux has
// `navigator` (navigator.d.ts) but no `navigator.clipboard`, so
// `"clipboard" in navigator` is the availability check.

interface Clipboard {
  /**
   * Resolves to the OS clipboard's text, "" when the clipboard is empty or
   * holds no text. Rejects on platform failure.
   */
  readText(): Promise<string>
  /**
   * Replaces the OS clipboard's contents with `text`. Rejects on platform
   * failure or a non-string argument.
   */
  writeText(text: string): Promise<void>
}

interface Navigator {
  readonly clipboard: Clipboard
}
