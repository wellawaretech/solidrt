---
title: sdl3 dialog callback data leaks per dialog and is typed without Send
description: sdl3::dialog boxes its callback data with Box::into_raw and never frees it, one leak per dialog shown; and DialogCallback is Box<dyn Fn> without Send although SDL may invoke it from another thread (Windows IFileDialog runs on a thread SDL spawns).
created: 2026-10-15
status: unfiled
project: sdl3 (Rust bindings)
versions: sdl3 0.18.4
link:
---

# sdl3 dialog callback data leaks per dialog and is typed without Send

`sdl3::dialog` (`show_open_file_dialog`, `show_save_file_dialog`,
`show_open_folder_dialog`, sdl3 0.18.4 `src/sdl3/dialog.rs`) hands SDL a
`DialogCallbackData` through `Box::into_raw` (the `callback_data_ptr!`
macro) and the `extern "C"` trampoline never reclaims it: the box, the
boxed `DialogCallback` and the filter `CString`s leak once per dialog
shown. Small per dialog, unbounded over a session.

`DialogCallback` is `Box<dyn Fn(Result<Vec<PathBuf>, DialogError>,
Option<DialogFileFilter>)>`, neither `Send` nor `FnOnce`, while SDL's
documentation for `SDL_ShowOpenFileDialog` says the callback may be invoked
from a different thread than the caller's (the Windows backend runs the
dialog on a thread of its own). A closure holding a `!Send` value compiles
and is then called off-thread.

Observed while building okf/done/file-dialogs.md (alloy dialog.rs): alloy
wraps its `FnOnce` responder in a `Mutex<Option<_>>` so the `Fn` contract
is met and the responder only ever holds `Send` state, which sidesteps the
second point; the leak is accepted for now.

Suggested upstream fix: free the data after the callback ran (the
trampoline is the last use; SDL calls it exactly once), and require
`Send` on `DialogCallback`, with `FnOnce` through a `Mutex<Option<_>>`
inside the binding so callers need no wrapper.
