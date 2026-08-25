---
title: Reload-on-save from the bundle's inputs, paused by the agent
description: The dev server watches the files the running bundle was built from (dependencies included) and the assets tree, rebuilding through the one reload path; an MCP pause_watch/resume_watch pair holds it while an agent edits, restored when the bridge exits.
created: 2026-08-25
completed: 2026-08-25
---

# What

- **`flux:fs` directory watch**: `dir(path).watch(callback, { recursive })`
  returns an unsubscribe. `forge::fs::DirWatcher` (notify-backed) is the
  core; the plugin routes the callback through the event bus under a
  per-watch event name (no JS value captured in a native closure), so the
  watch holds the engine alive until unsubscribed. Events are raw and
  undebounced, `{ kind: create | modify | remove | rename, path }`; a
  rename's target arrives as `rename` and its old name as `remove`, so an
  editor's atomic save is seen under the file's real name. A missing
  directory throws (the path is the caller's).
- **`BundleOutput.inputs`**: every file the bundle was built from, absolute
  and sorted: what the solid plugin loaded, every module in the build's
  sourcemap (which is where node_modules and workspace packages appear),
  files inlined by import attribute, and the project's package.json and
  tsconfig.json. The assets tree is not listed; the manifest carries it.
- **The server watcher** (`src/server/watcher.ts`): the parent directory of
  each input is watched non-recursively and events are matched by name
  within it (never the file itself: an atomic save replaces the inode).
  Project mode adds the whole `assets/` tree, since the manifest is that
  tree and any change to it is a new version, additions included. Hits
  debounce (100 ms) into `rebuildAndBroadcast()`, which re-arms the watch
  from the new inputs; a change landing during a build queues one more.
  While the last build failed the source directory is watched as a whole
  (the missing-import case). `load` re-arms through the same path.
- **Pause**: `POST /__control__/watch?active=false|true`, `watchPaused` in
  `/clients`, MCP `pause_watch`/`resume_watch`, the mute's shape exactly:
  two named tools, bridge restores on exit. Changes saved while paused are
  not replayed on resume; the agent's `reload` is the push.

# Why not

- **A directory watch.** It reloads on files the app does not import and
  misses the dependency the app does (a workspace package in this
  monorepo is the common case). The input set is the only honest answer
  to "what does the running app depend on", and the build already knows it.
- **An external watcher process.** Its only possible output is
  `POST /reload`, which any editor hook or `entr` can already send; the
  coordination with the agent lives in the server regardless, and a
  second process needs a launcher the flux migration just removed.
- **An implicit pause while a bridge is attached.** A bridge is spawned
  when a Claude Code window opens and lives until it closes (five were
  alive at the time of writing), and the server cannot see it anyway:
  every MCP call is one stateless HTTP request. "N seconds after any
  control call" is wrong-shaped too: the agent's cycle is edit, reload,
  verify (calls), edit again for minutes without a call, so the window
  expires exactly when the edit burst starts. Explicit it is.
- **Folding the pause into the mute.** The mute keeps the human out while
  the agent measures; the pause keeps the agent's own saves out while it
  edits. The windows are near-complementary; one hold would lock the human
  out for the agent's whole working period.

# Not done

- Asset changes in file mode (no project, no manifest assets): nothing to
  watch by design.
- A new `"use isolate"` module the entry does not import yet: the isolate
  set is found by scanning the tree at build time, so the file is picked up
  on the next rebuild (its importer's edit), not on its own creation.
