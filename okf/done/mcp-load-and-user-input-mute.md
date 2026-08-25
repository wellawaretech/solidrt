---
title: MCP load and user-input mute
description: Bring back the MCP load tool under the mode table, and give agents a mute on the user's own input on every client while they measure or test; named for the mechanism, not for a session.
created: 2026-08-25
completed: 2026-08-25
---

# What

- **`load`** returned (removed with the watcher in the srt rework):
  `POST /__control__/load { entry }` and the MCP `load` tool. A project
  server only loads files under its project root (the bundle needs the
  project's dependencies and assets, and the registry key keeps naming the
  project); a file server loads any file, moving the file routes and the
  bundler's cwd with the entry. The key never moves; `/clients` reports
  `entry` next to it.
- **User-input mute**: MCP `mute_user_input` / `unmute_user_input`,
  `POST /__control__/mute?active=`, latched in `state.userInputMuted`,
  sent to every client as `{ type: "mute", active }` and in `welcome` for a
  client joining while muted. On the client the alloy `App` owns
  `user_input_muted`; the pump reads it once per iteration and hands it to
  both input models: translated events (`event::is_muted_input`: moves,
  downs, wheel, text, key-down, back are dropped; releases pass so nothing
  sticks) and the level-read gamepads (`Gamepads::set_muted`: one neutral
  pad state on entry, no back edge, real state on exit). Resize, quit and
  visibility are not input. Cleared on connection loss, survives reload;
  the bridge unmutes on exit if it muted.

# Why the names

It started as `start_session` / `end_session`: "the moment an agent starts
working it announces itself, which mutes the user and would pause a file
watcher". Verifying it showed the concept is wrong: mid-work the agent needed
the human to press keys and a pad button, and "end_session" for that reads
as "I am done". What the tools do is mute and unmute the user's input, so
they are named for that; unmuting to let the human interact is then the
obvious move. A watcher pause, if a watcher returns, is a separate concern:
[backlog/reload-on-save.md](../backlog/reload-on-save.md).

# Why two mute sites

A gate at the SDL event drain looks like one place but does nothing for
gamepads: pad state is polled from SDL (`Pad::state`, `take_back_edge`),
not accumulated from events. So the flag is read once per pump iteration
and each input model applies it where its semantics live.

# Found on the way

- The lattice Makefile bundled the launcher with a file argument from the
  repo root, ambiguous under the mode table; it now runs in
  `apps/launcher` with `--project`.
- The launcher failed to start on the current core (BSOD on back-to-
  launcher): the per-node error containment `guard()` re-wrapped child
  accessors into arity-1 functions, and Solid's `flatten` only unwraps
  zero-arity functions, inserting anything else as a node. Fixed in
  `guard` (wrap only zero-arity values, keep arity 0). Any wrapper on the
  insert path must keep arity 0.
