// System file dialogs (gui-enabled runtime only): the platform's own open,
// save and folder pickers, resolving to flux:fs handles. Solidrt semantics
// on the web pickers' vocabulary: no permission model, one dialog at a time,
// a cancel is a value (an empty list or null), never an error. Listed in
// `Flux.capabilities` as "dialog" only where the platform can show one (a
// headless build, iOS, a Linux session without the portal or zenity, an
// Android without a document picker cannot), and a call there rejects with
// an error named "NotSupportedError". Under `sol test` no dialog is shown:
// the test answers it (`app.dialog`), and so can an agent over the control
// API, before the app asks; a test engine therefore lists "dialog" whatever
// its platform can show, so an app that gates on the capability asks there.

declare module "flux:dialog" {
  import type { FluxDir, FluxFile } from "flux:fs"

  /** A file type the dialog offers to narrow the listing to. */
  type FileType = {
    /** The label the dialog shows for it ("Images"); the extensions when omitted. */
    description?: string
    /**
     * The extensions it covers, without the dot (`["png", "jpg"]`; a leading
     * dot is tolerated). On Android each becomes a MIME type where the
     * platform knows one; a custom extension cannot be filtered on and
     * leaves every file pickable.
     */
    extensions: string[]
  }

  type OpenFilesOptions = {
    /** Let the user pick several files. Default false. */
    multiple?: boolean
    /** The file types offered; every file when omitted or empty. */
    types?: FileType[]
    /**
     * Where the dialog opens: a directory path (on Android a `content://`
     * URI; a plain path is ignored there). The platform's default when
     * omitted.
     */
    startIn?: string
  }

  type SaveFileOptions = {
    /** The file name pre-filled in the dialog. */
    suggestedName?: string
    /** The file types offered; every file when omitted or empty. */
    types?: FileType[]
    /** Where the dialog opens (see `OpenFilesOptions.startIn`). */
    startIn?: string
  }

  type OpenDirectoryOptions = {
    /** Where the dialog opens (see `OpenFilesOptions.startIn`). */
    startIn?: string
  }

  /**
   * Shows the open-file dialog and resolves to handles on the files the
   * user picked, in the order the platform reports them: one unless
   * `multiple`, and an empty array on a cancel. Each handle's `path` is a
   * path on desktop and a `content://` URI on Android (persisted, so the
   * string reopens the document after a restart), and its `name` is what
   * the picker showed. Rejects with an error named "NotSupportedError"
   * where no dialog can show, "InvalidStateError" while another dialog is
   * open, and a plain Error when the platform failed to show it.
   */
  export function openFiles(options?: OpenFilesOptions): Promise<FluxFile[]>
  /**
   * Shows the save-file dialog and resolves to a handle on the place and
   * name the user chose (the file need not exist yet; `write` creates it),
   * or null on a cancel. Writes replace the file's contents in place; for
   * a save that cannot leave a half-written document, pass `write(data, {
   * atomic: true })`. Rejections as for {@link openFiles}.
   */
  export function saveFile(options?: SaveFileOptions): Promise<FluxFile | null>
  /**
   * Shows the folder dialog and resolves to a handle on the folder the user
   * picked, or null on a cancel. Its `entries()` list the folder, each
   * entry carrying the `path` to open it with. On Android the platform
   * refuses the storage root and the Download folder for a folder pick
   * ("to protect your privacy"); the user picks a folder inside them.
   * Rejections as for {@link openFiles}.
   */
  export function openDirectory(options?: OpenDirectoryOptions): Promise<FluxDir | null>
}
