---
title: stdin/tty support in flux
description: A flux:tty module (terminal check, cooked lines or raw-mode keys, stdout write) bringing the srt dev-server repl back with history and Tab completion; both stages done, Windows and Android runs pending.
tags: [flux, tty, stdin, terminal, cli, repl]
created: 2026-07-13
---

# stdin/tty support in flux

The `srt run` / `srt server` repl went away with the
[CLI/flux migration](../backlog/cli-flux-migration.md): the dev server became
a flux script and flux had no stdin (the launcher even spawned it with stdin
ignored). This brings it back, as a capability of its own: anything that
wants an interactive terminal under flux (a repl, a TUI, a debugger prompt)
needs the same piece, so it is not scoped to the repl.

Checked before starting - there was nothing to reuse: `flux:process` has no
stdin; the `flux`/`fluxc` binaries read stdin once, blocking, as a script
source; `flux:subprocess`'s stdin is a spawned child's; SDL input is
window-event based.

# Shape

`flux:tty`, "the terminal attached to this process". Node's tty/readline
names, simplified semantics:

```
import { isTTY, on, setRawMode, write } from "flux:tty"
isTTY                 stdin is a terminal (a pipe, a file or no stdin: false)
on("line", cb)        one cooked line per delivery, newline stripped; returns off()
on("key", cb)         one key per delivery while raw mode is on: { name, char, ctrl, meta, shift }
on("close", cb)       stdin reached end of file (Ctrl-D, pipe closed)
setRawMode(on)        keys instead of lines, no echo, Ctrl-C is a key; applies from the next read
write(text)           stdout as is, no newline appended (the prompt)
```

Scoped by "is a terminal attached", not by OS: true on a dev machine's
shell, on Android from a terminal emulator, false for a GUI launch anywhere.

`on` is the `flux:process` `on()` shape on the shared event bus: the first
listener holds the engine loop, the last unsubscribe releases it (the
server's idle-is-exit shutdown keeps working). After EOF nothing can fire,
so the plugin drops every tty listener itself.

# Stages

1. DONE 2026-08-26 (uncommitted): cooked mode, repl back with the old
   commands. No new crates.
   - `forge/src/tty.rs`: `is_terminal` (`std::io::IsTerminal`), `open_lines`
     (a process-wide reader thread feeding a channel; stdin is one stream
     per process, so the second caller gets `None`), `write`.
   - `flux/src/forge_plugins/tty.rs`: the bus wiring, a copy of the signal
     watcher pattern; `flux:tty` registered, `"tty"` in the capabilities;
     `ListenerRegistry::clear` added for the EOF release.
   - `packages/flux-types/modules/tty.d.ts`.
   - Launcher passes stdin through (`packages/cli/src/main.ts`).
   - `packages/cli/src/server/repl.ts`: `load`, `reload [id...]`,
     `stop [id...]`, `list`, `stats [on|off]`, `watch on|off`, `mute on|off`,
     `quit`, `help`. Clients are addressed by the id `list` prints (the MCP
     ids), not the old positional index. The actions are the control API's,
     factored into shared functions (`loadEntry`, `setUserInputMuted`,
     `setWatchActive`, `setStats` in control.ts). Started after the initial
     bundle; shutdown detaches it; Ctrl-D quits.
   - Not carried over: the `!<shell>` escape.
   - Accepted: server log lines interleave with the prompt line (the old
     bun readline did not redraw either).
2. DONE 2026-08-26 (uncommitted): raw mode and line editing.
   - `crossterm` 0.27 in forge (default-features off, `windows` + `events`;
     no event-stream, so no futures dep): `set_raw_mode`, `restore`, and
     the reader thread reads one key per iteration while raw (blocking
     `event::read()`), a cooked line otherwise; the mode applies from the
     next read. Node keypress names. Not `rustyline`: it owns the whole
     blocking loop, which fights the thin-plugin layering; JS does the
     editing.
   - The flux binary logs through `forge::tty::write_line` and restores the
     terminal at exit and from a panic hook.
   - `packages/cli/src/server/line-editor.ts`: buffer + cursor
     (left/right/home/end, Ctrl-A/E/U, backspace/delete), session history
     (Up/Down), Tab completion (commands; `load` paths via `flux:fs`
     entries, relative to what loadEntry resolves against), Ctrl-C and
     Ctrl-D on an empty line quit. console.* is wrapped while the editor is
     up: the prompt line is cleared before a message and redrawn after (the
     former stage 3, pulled in because raw mode has no echo). A refused raw
     mode falls back to the stage-1 cooked path.
   - `isTTY` is false for a job backgrounded from an interactive shell
     (`srt run &`), which job control would otherwise stop on the first
     terminal write: [notes/terminal-raw-mode](../notes/terminal-raw-mode.md).
   - Verified under a pty (`script`): keys, `\r\n` breaks, completion,
     history, Ctrl-U, Ctrl-C shutdown with the record dropped. Windows and
     Android: compile-only, see Open below.

# Findings

Cut to [notes/terminal-raw-mode](../notes/terminal-raw-mode.md): raw mode
and output post-processing, the stdin reader thread, scripted pty tests.

# Open

The Windows and Android runs:
[backlog/tty-repl-platform-runs](../backlog/tty-repl-platform-runs.md).
