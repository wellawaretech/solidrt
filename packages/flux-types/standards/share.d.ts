// navigator.share: the platform's share sheet, which hands files and a text
// to another app the user picks (a messenger, mail, a cloud drive). Solidrt
// semantics on the web's shape: no user-gesture rule (one known app), no
// canShare() (the "share" capability says it), and `files` holds flux:fs
// file handles (flux has no File). Android only, for now. `navigator.share`
// is defined, and "share" is in `Flux.capabilities`, only where the
// platform can show a sheet, so `if (navigator.share)` is the check; under
// `sol test` it is defined everywhere, and a share is recorded instead of a
// sheet showing (`app.requestedShares()`). A dev client shows the sheet and
// records the share too, for an agent to read back.

type ShareData = {
  /** The subject a target may use (a mail's subject line). */
  title?: string
  text?: string
  /** Sent after the text, on a line of its own. */
  url?: string
  /**
   * Files to hand over: `file()` handles of files in app storage, or of
   * documents a file dialog picked. The target gets a reference, not the
   * bytes, and reads them when it likes (a messenger uploads in the
   * background, a mail app reads at send), so a shared file must stay in
   * place after the call resolves: reuse a fixed name the next share
   * overwrites, or clean up much later, never right after the `await`.
   */
  files?: import("flux:fs").FluxFile[]
}

interface Navigator {
  /**
   * Opens the platform's share sheet with `data`. Resolves once the sheet
   * accepted the share: on Android, when the user picked a target app or
   * used one of the sheet's own actions (Copy). Rejects with an
   * "AbortError" when the user closed the sheet without picking, where the
   * platform reports a dismissal (Android does); with an
   * "InvalidStateError" while another sheet is open. Says nothing about
   * whether the target app finished the send.
   *
   * Throws a TypeError for data with nothing to share, a field of the
   * wrong type, or a file that is not a flux:fs handle; rejects with one
   * for a file outside app storage that no file dialog picked.
   *
   * Undefined where the platform has no share sheet.
   *
   * ```ts
   * let png = file("share/board.png")
   * await png.write(encodeImage(await captureSnapshot(boardNode)))
   * try {
   *   await navigator.share!({ text: "My board", files: [png] })
   * } catch (e) {
   *   if ((e as Error).name !== "AbortError") throw e
   * }
   * ```
   */
  share?(data: ShareData): Promise<void>
}
