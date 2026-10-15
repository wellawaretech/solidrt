---
title: Control-API queries stall after the Android picker returns a pick
description: On the tablet, after a file dialog pick that kept the app behind DocumentsUI for minutes, queries to the client (dialog state, debug list, settle) timed out for tens of seconds and then recovered; a BACK-cancelled picker and content reads alone did not stall; seen twice, cause not found.
created: 2026-10-15
---

# Control-API queries stall after the Android picker returns a pick

What it looks like when hit: after a `flux:dialog` pick on the tablet
(SM-T500, Android 12, dev client over adb reverse), the server's queries
to that client (`/dialog`, `/debug` list, `/settle`) answer "Query timed
out after 10000 ms: the client is connected but did not answer" for tens
of seconds, then answer again as if nothing happened. The app itself is
fine throughout: the pick's promise resolved, the document was read through
forge's content arm, the status line was drawn (logcat shows the frame),
and the client's own log lines reached the server at once.

Seen twice on 2026-10-15 while verifying okf/done/file-dialogs.md's
placement refactor, each time after a non-empty pick that had the app in
the background for minutes (the human picking took a while). Not seen:

- after a picker cancelled with BACK a few seconds after it opened (the
  same hidden/visible round trip, the same first frame on return; queries
  answered within a second);
- after a reread of the same `content://` URI with no picker (display name
  on the JS thread, stat and bytes on the blocking pool, 5.7 MB; queries
  answered in about 4 ms throughout the read).

So the dialog session and the content reads are cleared; what differs is
the long time hidden behind another activity. Candidates: the app frozen
or cached by the platform while hidden (Android 12 cached-app freezer,
Samsung's own policies) with the websocket or the engine's wake path
needing a while to recover; or the engine's suspend hold not released
promptly on this return path. The `[sol] suspend` line is logged when the
picker opens and nothing is logged for the resume, only `visibility:
hidden` / `visible`.

## Done looks like

- Reproduced with a controlled hide time: open the picker, leave the app
  hidden for one, three and five minutes, pick, then poll `/dialog` and
  `/debug` every second; the stall's length against the hide time says
  whether it is the platform's freezer.
- The cause named, and queries answering within a second of the return.

## What it involves

A probe app with a `registerDebug` command that calls `openFiles()`, the
dev client over USB (`adb reverse tcp:P tcp:P`, `am start -S ... --es
sol_dev_server 127.0.0.1:P`; over WiFi the tablet's socket drops behind
the picker and the reconnect rebuilds the engine, losing the pending
dialog), a human pick, and a one-second poll of two queries around it.
