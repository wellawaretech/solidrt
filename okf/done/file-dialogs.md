---
title: System open and save dialogs
description: Built 2026-10-15 as flux:dialog (openFiles, saveFile, openDirectory over SDL's dialogs on desktop and the activity's own document intents on Android), resolving to flux:fs handles that carry a name; a content provider seam in forge reads and writes content URIs; tests and agents answer a dialog through one lattice queue (app.dialog, /dialog, get_dialog/answer_dialog); write(data, { atomic }) for safe saves.
created: 2026-10-14
completed: 2026-10-15
---

# System open and save dialogs

Nothing in `flux:*` or `@solidrt/core` opened a native file dialog. An app
could read and write its storage folder (relative `flux:fs` paths) or
absolute paths it already knew, so "open a document from anywhere" and
"save this export where another program will find it" were impossible: the
user had to dig the file out of the app's storage folder by hand, and on
Android the storage folder was not reachable at all without adb.

## Done looks like

- An app can ask the user to pick one or more files to open, a place and
  name to save to, and a folder, through the platform's own dialog, on
  Linux, Windows, macOS and Android.
- What comes back can be read and written with the same ease as a
  `flux:fs` file, on Android too, where the answer is a `content://` URI
  and not a path.
- A cancel is a distinguishable outcome, not an error the app has to parse.
- App tests and agents (MCP / control API) can drive a flow that opens a
  dialog, without a human clicking it.
- Where no dialog exists (no portal/zenity on Linux, no document picker on
  an Android TV, iOS today), the app can tell before or when it asks.

## What was built

**Surface: `flux:dialog`**, its own module rather than functions on
`flux:fs`, so the next system dialog (a message box) has a home without a
rename, and the module marshals alloy from `alloy_plugins/`, installed by
`gui::install` like `navigator.clipboard`. `openFiles({ multiple, types,
startIn })` resolves to `FluxFile[]` (empty on a cancel), `saveFile({
suggestedName, types, startIn })` and `openDirectory({ startIn })` to a
handle or null. The option words are the web pickers'; the result is the
runtime's one file model, not a second handle type. `types` is `[{
description, extensions }]`. A second call while one dialog is open rejects
with an `InvalidStateError`; a platform without a dialog rejects with a
`NotSupportedError`, and the capability `"dialog"` is listed in
`Flux.capabilities` only where `alloy::dialogs_available()` says the
platform can show one: never on a headless surface, iOS, a Linux session
with neither the portal service nor zenity, or an Android without a
document picker.

**Handles carry a `name`** (`FluxFile.name`, `FluxDir.name`): the file name
of a path, a provider's display name on Android. A picked handle's `path`
is a path on desktop and a persisted `content://` URI on Android, so the
string the app stores reopens the document after a restart. Directory
entries carry the `path` that opens them, which on a picked Android folder
is the child's tree-scoped document URI.

**alloy** (`dialog.rs`): `AlloyCommand::ShowDialog { request, respond }`
handled on the loop thread with the app window as parent through
`sdl3::dialog`; the responder fires from SDL's callback (a cancel is an
empty answer). The stepped loop answers with an error at once. Android goes
through `SolidRTActivity.showDialog` over JNI and comes back through
`nativeDialogResult`, because SDL's own Android backend takes no persistable
grant on what it picked and has no folder picker (checked in
`SDLActivity.showFileDialog`). The activity maps extensions to MIME types
and keeps every file pickable when it does not know one, instead of
silently dropping the type as SDL does.

**forge** (`fs.rs`, `fs/content.rs`): a `content://` location resolves
through the platform's ContentResolver, on Android a `cfg` arm of forge's
own (JNI into the activity's `contentOpen`/`contentStat`/
`contentDisplayName`/`contentChildren`/`contentDelete`, over the
process-wide JavaVM and activity alloy publishes in ndk-context at
startup); `read`, `read_range`, `write`, `append`, `remove`, `stat`,
`exists`, `read_dir`, `open_seekable` and `display_name` route to it, and
what has no meaning on a URI (`rename`, `realpath`, `create_dir`) errors as
"not a path". Off Android a content URI resolves nowhere. Built first as a
`ContentProvider` trait in forge, implemented by an `alloy::AndroidContent`
and adapted in lattice; folded into forge the same day under the
alloy/forge placement rule now in CLAUDE.md (Projects): the resolver needs
no SDL, so it is forge's, and the trait, the global and the adapter went.

**Atomic saves**: `write(data, { atomic: true })` writes a temporary file
beside the target and renames it over; in place stays the default and the
primitive. On a content URI there is no rename, so `atomic` writes in place
with truncation (`wt`), as documented on the type.

**Tests and agents**, one mechanism: `alloy::Dialogs`, the session the
plugin keeps per engine (where a test takes the platform's place, as with
a test microphone or a still camera), holds the answer queue; an answer
queued ahead is consumed by the next call instead of a dialog, an answer
arriving while a call waits settles it at once (a native dialog still open
then has its own answer dropped, since SDL cannot dismiss it), and under a
test host (`set_answers_only`) a call never shows anything but waits on the
queue, holding the engine as work in flight so `settle` reports it. The
plugin marshals only: options in, handles out, a rejection named for the
session's refusal. The JSON of the control API and the test verb is
lattice's (`lattice/src/dialog.rs`). `app.dialog(answer)` in `@solidrt/test`
resolves, once the app's call took the answer, to the request the app made
(kind and options), so a test asserts what the app asked for. The control
API has `/dialog` (GET the pending request, the last one with its answer,
the queue length and the availability; POST an answer) and the MCP tools
`get_dialog` and `answer_dialog`.

## Verified

- Desktop (Linux, Hyprland, xdg-desktop-portal-gtk): the capability listed
  on a live client; an answer queued over `/dialog` consumed by a
  debug-triggered `openFiles` (handles named and readable); the native
  Open File, Save File and Open Folder dialogs showing as portal windows
  for an unanswered request, `/dialog` reporting it pending and `/settle`
  reporting `inFlight: { dialog: 1 }`; closing the dialog window resolving
  the call as a cancel; an answer over `/dialog` while a native dialog was
  open resolving the call at once, with the dialog's own later cancel
  dropped.
- `packages/core/tests/dialog.test.tsx` (8 tests): queue ahead, wait for
  the answer, named answers, save and write back (atomic), folder entries
  with paths, one at a time, option validation, no capability under the
  headless test client. `flux/tests/write.test.ts` covers the atomic write,
  `flux/tests/dir.test.ts` the entry paths, `lattice/src/tests/dialog.rs`
  the answer parser and the request and status JSON,
  `alloy/src/tests/dialog.rs` the filter pattern, the names and the
  session (queue ahead, wait, refuse, the platform's late answer dropped).
- Android (SM-T500, Android 12, dev client over adb reverse): the
  capability listed; `openFiles` showing DocumentsUI, a pick from Downloads
  coming back as a `content://` URI named by the provider, read and stat'd
  through the content provider seam; `saveFile` with the suggested name
  pre-filled, the created document written through and visible in
  Downloads over adb; `openDirectory` on Documents (Android refuses the
  root and Download for tree picks) listing its children as tree-scoped
  document URIs that read; back out of the picker resolving as a cancel;
  all three stored URIs readable after a force-stop and relaunch, so the
  persistable grants hold. Two findings on the way: the activity's
  `dialogsAvailable`/`showDialog` are static (`call_static_method`), and
  package visibility hides the picker from `resolveActivity` without a
  `<queries>` block in the manifest. What no device here can show, a
  platform without a picker (the TV), is open:
  okf/backlog/file-dialogs-no-picker-device.md.

## Decisions

- No web File System Access layer (`showOpenFilePicker` and its
  `FileSystemFileHandle`/`Blob`/`File`): Chromium-only, and it would put a
  second handle type for the same file next to `FluxFile`. A real `Blob`
  and `File` are worth their own item (for `Response.blob()` and friends).
- Recent files are a string the app keeps, not a runtime-kept list: the
  handle reduces to its `path`, and on Android the activity takes a
  persistable grant at pick time (capped per app by the platform).
- Sharing (`navigator.share`, an Android share intent) stays out: a save
  dialog already gets a file to where another program can open it.
- iOS has no SDL backend; a `UIDocumentPickerViewController` path belongs
  with okf/backlog/ios-port-readiness.md.
