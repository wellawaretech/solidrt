---
title: stdin/tty support in flux
description: A flux:tty module (terminal check, line input, raw stdout write) bringing the srt dev-server repl back, staged from cooked-mode lines to raw-mode line editing.
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
import { isTTY, on, write } from "flux:tty"
isTTY                 stdin is a terminal (a pipe, a file or no stdin: false)
on("line", cb)        one cooked line per delivery, newline stripped; returns off()
on("close", cb)       stdin reached end of file (Ctrl-D, pipe closed)
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
2. Raw mode and line editing: `crossterm` (event-stream feature) in forge,
   `setRawMode(bool)` and `on("key", cb)` with `{ name, ctrl, shift, char }`,
   terminal state restored on exit and panic; a JS line editor in the CLI
   (buffer + cursor, history, the old `completer()` on Tab, Ctrl-C/Ctrl-D).
   Not `rustyline`: it owns the whole blocking loop, which fights the
   thin-plugin layering; crossterm's key stream is the right primitive, JS
   does the editing. Android goes through `libc` termios like any Unix; a
   verification run, not an assumption.
3. Optional: a clean prompt over logs (clear the prompt line before a log
   line, redraw after). Needs the server's log output to go through the tty
   writer; only if stage 2 shows it matters.

# Findings

- Read stdin on a plain OS thread, never `tokio::io::stdin()`: that reads on
  the blocking pool, and a runtime being dropped waits for its blocking
  tasks, so a read parked on stdin hangs the process at exit. A detached
  thread dies with the process.
- The event bus is one namespace per context, so the tty events go on it as
  `tty:line` / `tty:close` while the JS names stay `line` / `close`.
