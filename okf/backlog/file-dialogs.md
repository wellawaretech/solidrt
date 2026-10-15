---
title: System open and save dialogs
description: An app can only read and write inside its own storage folder or at absolute paths it already knows; there is no native open/save dialog, so a user cannot open a document from anywhere or save an export where another program will find it. SDL 3.4 has the dialogs; the open questions are the JS surface, Android content URIs, and how tests and agents answer a dialog.
created: 2026-10-14
---

# System open and save dialogs

Nothing in `flux:*` or `@solidrt/core` opens a native file dialog. An app
reads and writes its storage folder (relative `flux:fs` paths) or absolute
paths it already knows, so "open a document from anywhere" and "save this
export where another program will find it" are impossible: the user has to
dig the file out of the app's storage folder by hand. On Android
it is worse: the storage folder is not reachable at all without adb.

## Done looks like

- An app can ask the user to pick one or more files to open, and a place
  and name to save to, through the platform's own dialog, on Linux,
  Windows, macOS and Android.
- What comes back can be read and written with the same ease as a
  `flux:fs` file, on Android too, where the answer is a `content://` URI and
  not a path.
- A cancel is a distinguishable outcome, not an error the app has to parse.
- App tests and agents (MCP / control API) can drive a flow that opens a
  dialog, without a human clicking it.
- Where no dialog exists (no portal/zenity on Linux, no document picker on
  an Android TV, iOS today), the app can tell before or when it asks.

## What exists

- **SDL 3.4 (we are on 3.4.10) has the dialogs**: `SDL_ShowOpenFileDialog`,
  `SDL_ShowSaveFileDialog`, `SDL_ShowOpenFolderDialog`. Backends: Linux via
  the xdg-desktop-portal FileChooser over D-Bus with a zenity fallback,
  Windows IFileDialog (on a thread SDL spawns), macOS NSOpenPanel (a sheet
  on the parent window, `runModal` without one), Android
  `ACTION_OPEN_DOCUMENT` / `ACTION_CREATE_DOCUMENT` (our copy of
  `SDLActivity.java` carries `showFileDialog` and the `onActivityResult`
  side). iOS has only the dummy backend.
- **The sdl3 crate (0.18.4) wraps them** in `sdl3::dialog`
  (`show_open_file_dialog`, `show_save_file_dialog`,
  `show_open_folder_dialog`), so no FFI of our own. Its callback type is a
  plain `Box<dyn Fn>`, not `Send`, although SDL may invoke it on another
  thread; ours only feeds a oneshot, which is `Send` anyway.
- **The thread seam exists.** SDL wants the call on the main thread; JS
  runs on sol-ui. `navigator.clipboard`
  (flux/src/standards_plugins/clipboard.rs) already does the round trip: an
  `AlloyCommand` with a responder, the responder feeds a oneshot, the
  promise awaits it, installed from `gui::install` so it is absent on a
  headless build. A dialog is the same, except the responder fires later
  from SDL's callback.
- **Android content URIs can be opened**: our `SDLActivity` has a public
  static `openFileDescriptor(uri, mode)` (ContentResolver), and alloy already
  depends on `jni`. SDL's own `SDL_IOFromFile("content://...")` uses the same
  route internally and translates fopen modes to ContentResolver ones (`w`
  becomes `wt`; a bare `w` does not truncate on some providers since
  Android 10, so a shorter save would keep the old tail).

## What it involves, whatever the options

- alloy: one `AlloyCommand` (kind, filters, default location or suggested
  name, allow-many, responder) handled on main with the app window as the
  parent (modal sheet on macOS, not `runModal` blocking the main loop).
- flux: the binding in `standards_plugins/` (if web-shaped) or in
  `forge_plugins/` (if a flux:fs addition), plus `packages/flux-types`.
- Android: reading `content://` (see option 3), and a display name for the
  picked document: a URI has no file name, so `name` needs a
  ContentResolver query for `OpenableColumns.DISPLAY_NAME` (a small helper in
  `SolidRTActivity`, called over JNI).
- Filters: SDL takes `{ name, pattern: "png;jpg" }`. On Android SDL maps each
  extension to a MIME type through `MimeTypeMap` and drops the ones it does
  not know, so a custom extension falls back to "any file". Document it.
- Tests and agents: see option 7.

## Options (undecided)

### 1. JS surface

- **a. The web File System Access API.** Globals
  `showOpenFilePicker({ types, multiple })`,
  `showSaveFilePicker({ suggestedName, types })` and (option 5)
  `showDirectoryPicker()`, returning `FileSystemFileHandle`s with `kind`,
  `name`, `getFile()` and `createWritable()`; a cancel rejects with an
  `AbortError`. SolidRT simplifications: no permission model
  (`queryPermission`/`requestPermission` absent or always `"granted"`),
  `types[].accept` collapsed into SDL's extension filters, `id`/`startIn`
  dropped or `startIn` as a path. Web-trained code and coding LLMs guess it
  right; costs `Blob`/`File` (option 2) and a writable stream class.
- **b. A flux:fs addition.** `pickFiles(options)` and
  `pickSaveFile(options)` in `flux:fs`, resolving to `FluxFile` handles (the
  `file()` references apps already use: `text()`, `bytes()`, `write()`,
  ...), or to an empty list / null on cancel. Small, no new standard
  classes; nobody guesses the names.
- **c. Both, layered.** b as the primitive (a `FluxFile` that can stand on
  a `content://` URI), a as a thin standard layer over it in
  `standards_plugins/`.

### 2. `Blob` and `File` (only with 1a or 1c)

flux has neither; `getFile()` returns a `File`.

- **a. A minimal real `Blob`/`File`** in `standards_plugins/`: `size`,
  `type`, `name`, `lastModified`, `text()`, `arrayBuffer()`, `bytes()`,
  `stream()`, `slice()`. Also unlocks `Response.blob()` and friends later.
- **b. A File-like object** with the read methods but no `Blob` class
  (`instanceof Blob` fails). Less work, a trap for code that checks.

### 3. Android `content://` reaching forge

`forge::fs` is `std::fs` on paths; `file("content://...")` fails today.

- **a. An opener hook in forge**: forge stays engine- and SDL-free and
  asks a platform-installed opener for any `content://` path; lattice
  installs one on Android that calls `openFileDescriptor` over JNI and
  returns a `std::fs::File` from the fd. Every `flux:fs` file method then
  works on a URI (except the path-shaped ones: `rename`, `realpath`, `dir`).
- **b. The handle carries the open fd** from pick time (one fd per handle,
  reopened for writes). No forge change; handles become stateful and hold
  a descriptor for their lifetime.
- **c. Go through `SDL_IOFromFile`** in alloy and read/write via
  `SDL_IOStream`. SDL already handles modes; but forge's file code would
  need an IO abstraction it does not have, and the bytes cross alloy.

### 4. Write semantics

- **a. Atomic, as the web does**: `createWritable()` writes a temp file
  and `close()` renames it over the target. Possible on desktop paths; not
  on a `content://` URI (no rename), which writes in place with `wt`.
  Two behaviours to document.
- **b. In place everywhere**: truncate and write. One behaviour; a crash
  mid-save leaves a truncated file.

### 5. Directory picker

- **a. In scope now.** Desktop answers a path, so `dir()` just works.
  Android answers a tree URI (`ACTION_OPEN_DOCUMENT_TREE`), listed through
  `DocumentsContract` child queries, a separate read path from option 3.
- **b. Later.** Files only; the folder dialog waits for a real need.

### 6. Handles across restarts ("recent files")

- **a. None.** Pick every time. A desktop app can still keep the path
  string it got.
- **b. A handle reduces to a string** (path or URI) the app stores itself
  and reopens later. Android needs `takePersistableUriPermission` at pick
  time (SDL does not call it; ours to add in the activity), and grants are
  capped per app.
- **c. A runtime-kept recent list.** Hidden state; against the
  primitive-first grain unless opt-in.

### 7. Tests and agents answering a dialog

The headless test client cannot show a dialog, and an agent cannot click
one. App code keeps calling the real function in every option.

- **a. A harness verb** (an `app.` call in `@solidrt/test`, like
  `app.camera`) that queues the answer to the next dialog: paths inside the
  test sandbox, or a cancel.
- **b. A control API endpoint** (`/dialog`: list the pending dialog with
  its kind and filters, answer it with paths or cancel) and an MCP tool on
  top. The app test harness could use the same mechanism, so a and b may
  be one thing.
- **c. A dev-mode policy**: under test or headless, dialogs resolve from a
  fixed rule (always cancel, or a configured folder). Simplest; cannot
  express "the user picked these two files" per step.

### 8. Availability

- **a. Reject at call time** with a `NotSupportedError` when the platform
  has no dialog (no portal/zenity, Android without a document picker,
  iOS).
- **b. A `Flux.capabilities` entry**, so an app can hide its Open/Save
  buttons up front, plus 8a on top for the cases only a call discovers
  (a portal that is installed but fails).

## Out of scope

- Sharing (`navigator.share({ files })`, an Android share intent with a
  FileProvider). A save dialog already gets a file to where another program
  can open it, on desktop and on Android.
- iOS: no SDL backend; a `UIDocumentPickerViewController` path belongs with
  okf/backlog/ios-port-readiness.md.
