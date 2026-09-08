---
title: An asset-generating script reloads the app on every write
description: The dev server watches the assets/ tree with a 100 ms debounce, so a data-prep script writing hundreds of MB over two minutes rebuilds and pushes the app on every file, and the client thrashes reloading half-written data; the watch should settle a burst instead of chasing it.
created: 2026-09-08
---

# An asset-generating script reloads the app on every write

## Symptom

A prep script writes 229 MB of point data into `assets/` as four files
over about two minutes. The dev server watches the assets tree, so each
file lands, the 100 ms debounce expires, the bundle rebuilds and every
client reloads - and the app then spends seconds loading half a
gigabyte of data before the next write restarts it. From the human's
side the window simply thrashes.

Nothing reports this. The reloads produce no error and no log line, so
an agent driving the app sees no sign that it restarted twenty times;
`generation` in `list_clients`/`get_logs` is the only tell, and only if
you happen to look.

## Cause

`packages/cli/src/server/watcher.ts`: `DEBOUNCE_MS` is 100, which is
tuned for an editor's save (one atomic rename, push immediately). A
generated file is the opposite shape - a long write, then another file,
then another - and 100 ms is short enough that the watch fires inside
the burst, repeatedly, on data that is not finished being written.

`pause_watch` is the documented answer and it works, but it needs an
agent to connect "running a data-prep script" with "editing", which the
tool's wording did not invite (widened 2026-09-08). A human running the
same script gets no such affordance at all.

## Done looks like

A burst of asset writes produces ONE rebuild, after the burst:

- Asset-tree hits settle on a longer window than source hits (a second
  or two), so a stream of writes coalesces instead of firing per file.
  The bundle's own input list keeps the snappy debounce - that path is
  an editor save and should stay immediate.
- Better if cheap: do not rebuild on a file whose size is still
  changing (stat, compare, wait one more window). That covers a single
  large file, which the longer window alone does not.

Not in scope: making the reload itself cheaper. An app reloading half a
gigabyte is doing what it was asked to.

## What it involves

`watcher.ts` alone: one more timer constant and a per-path branch on
whether the hit came from the assets watch or the input watch (they are
already separate `dir().watch()` registrations).
