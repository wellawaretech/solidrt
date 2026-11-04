---
title: The first control-API query after an Android launch can time out
description: On the tablet, the first query to a freshly launched dev client sometimes times out at 10 s while the next one on the same client answers in milliseconds; seen on 2 of 11 launches, once during the first-connect asset download and once with no download, cause not found; a query landing before the app's entry ran is the leading suspect.
created: 2026-11-04
---

# The first control-API query after an Android launch can time out

What it looks like when hit: right after `am start -S` of the dev client on
the tablet, once `/__control__/clients` lists the new client with
`loading: false`, the first query to it (`POST /debug?name=<cmd>`) answers
"Query timed out after 10000 ms: the client is connected but did not
answer". The next query to the same client, sent after the timeout, answers
in about 13 ms, and everything after that is normal. An agent that reads
the first timeout as "the app is wedged" goes looking in the wrong place.

Seen on 2026-11-04 (SM-T500, Android 12, dev client over WiFi with `sol run
--lan`), verifying `openLink` (okf/plans/ore-landing-zone.md) with a probe
app whose debug commands read `navigator.languages` and start an
`openLink`:

- Launch 1, the first connect after `adb install -r`: the server log shows
  the client fetching the repo root's assets right after connecting
  (`lizard.sol3s` 57 MB, `lizard-pq.sol3s` 25 MB, `train.sol3s` 25 MB, the
  audio files). The first query timed out.
- Launch 2, started while Firefox was in front: no asset fetch logged. The
  first query timed out.
- Not seen in 9 more launches: 5 with the same query first (90 to 205 ms
  for it), 1 started from behind Firefox, the others with another query
  first.

Ruled out: the query itself. Its command reads `navigator.languages`,
whose Android side (`SDLActivity.getPreferredLocales`) only formats
`LocaleList.getAdjustedDefault()`, and the same command answered in 15 ms
on every other launch.

The leading suspect is the startup window. `loading: false` does not mean
the app's entry has run: on one launch a query sent at that point answered
`no debug command 'locale' (none registered)`. A client also starts more
than one engine at startup (the desktop client's log shows the
`apps/default` engine, then the app's `apps/index` engine). A query
exec'd on an engine that is dropped before it runs the closure would
never be answered, which is exactly a timeout. The first-connect asset
download (launch 1) would widen that window. Unconfirmed.

Possibly the same family as okf/backlog/android-query-stall-after-picker.md
(queries stall after the app was hidden, then recover).

## Done looks like

- Reproduced on purpose: a script that relaunches the client and sends one
  query the moment `/clients` shows it, run 20 or more times over WiFi and
  over USB (`adb reverse`), and once more after clearing the client's
  asset cache. The rate says whether it is the startup window.
- The cause named.
- A query that lands in the startup window either answers or fails at once
  with the reason ("no running engine", "the app is still starting"),
  never with a 10 s timeout.
- `/clients` (and `list_clients`) says when the app's entry has run, so a
  caller knows when a debug command can be reached.

## What it involves

`lattice/src/go/connection.rs` (the query dispatch: `queries.exec`, set as
each engine starts and read for each query) and the engine swap at client
startup in `lattice/src/lib.rs`; the `loading` field `/clients` reports
(packages/cli/src/server/control.ts). A probe app with one debug command
and the tablet; the loop and the launch command are in
packages/cli/agents/debugging.md and the Android device notes.
