---
title: Terminal input and raw mode under flux
description: What a process that reads its own terminal has to know - crossterm's raw mode clears output post-processing on Unix (bare newlines staircase), stdin must be read on a plain OS thread rather than tokio's blocking pool, and a scripted pty test must not feed control bytes before the process has switched to raw mode.
tags: [flux, tty, terminal, testing]
created: 2026-08-26
---

# Terminal input and raw mode under flux

Found while building `flux:tty` ([done/stdin-tty-support](../done/stdin-tty-support.md)).
The first two facts are also stated at the code that carries them
(`forge/src/tty.rs`); this note holds the reasoning and the test recipe.

## Raw mode kills output post-processing

crossterm's `enable_raw_mode` is `cfmakeraw`. On Unix that clears `OPOST`
along with echo, canonical input and signal keys, so a bare `\n` no longer
returns the carriage: every `println!` in the process staircases, not just
the editor's own output. Node's `setRawMode` (libuv) keeps `ONLCR` on, which
is why a readline-based repl never sees this. Under flux the logger path
writes `\r\n` while raw mode is on (`forge::tty::write_line`, the `flux` and
`fluxrt` binaries) and anything else writing to the terminal in raw mode
must do the same. The Windows console is unaffected (output processing is a
separate flag crossterm leaves alone).

Raw mode is terminal state, not process state: it survives the process. A
normal exit and a panic restore it (the binaries' hook); `kill -9` does not,
and `reset` at the shell fixes it. The same is true of every TUI.

## A background job must not touch the terminal

`cmd &` in an interactive shell leaves the terminal as stdin (only a
non-interactive shell substitutes `/dev/null`), so "stdin is a terminal" is
true for a job that is not the terminal's foreground process group. Such a
job is stopped by job control the moment it changes terminal settings
(SIGTTOU, `Stopped (tty output)` in `jobs`) or reads (SIGTTIN); a stopped
process holds every signal but SIGKILL/SIGCONT, so `kill %1` does nothing
until the shell exits and SIGHUPs it, without a clean shutdown. Reproduced
2026-08-26 with `srt server &`. The check is `tcgetpgrp(stdin) == getpgrp()`
(rustix, safe), folded into `forge::tty::is_terminal`: a terminal we would
be stopped for touching counts as none, and the server runs without a repl
as it does under a supervisor. Windows has no job control.

## stdin is read on a plain thread

`tokio::io::stdin()` reads on the blocking pool, and a runtime being dropped
waits for its blocking tasks, so a read parked on stdin hangs the process at
exit. A detached `std::thread` dies with the process. stdin is one stream per
process, so there is one reader thread; the engine that opens it keeps the
channel end and parks it between subscriptions.

## Scripted pty tests

`script -qec "<cmd>" /dev/null` gives the command a pty and forwards the
bytes on script's stdin to it. Two traps:

- Bytes fed before the process switches to raw mode go through the cooked
  line discipline: `\x03` there is SIGINT to the foreground process group,
  `\x04` is EOF, and a line is only delivered on `\n`. Sleep before typing
  (the dev server takes a few seconds to bundle before its prompt), and never
  send `\x03`/`\x04` early.
- A pipeline waits for its slowest member: `(sleep 5) | cmd` measures the
  sleep, not `cmd`. Time inside the right-hand side.

Working feed for the dev-server repl, from a project directory:

```
(sleep 12; printf 'hel\t\r'; sleep 0.5; printf '\x1b[A\r'; sleep 0.5; printf '\x03'; sleep 4) \
  | script -qec "bunx srt server" /dev/null
```
