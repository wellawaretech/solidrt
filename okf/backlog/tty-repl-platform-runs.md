---
title: The dev-server repl has only run on Linux
description: flux:tty raw mode and the srt repl are crossterm-backed and compile for Windows and Android, but neither has been run there - the Windows console ANSI path and Android termios from a terminal emulator are unverified.
tags: [flux, tty, cli, windows, android]
created: 2026-08-26
---

# The dev-server repl has only run on Linux

`flux:tty` ([done/stdin-tty-support](../done/stdin-tty-support.md)) is
verified under a Linux pty. On the other platforms it compiles and nothing
more.

- **Windows.** crossterm's raw mode goes through the Console API; the
  editor's cursor and clear-line sequences are ANSI, enabled by asking
  `crossterm::ansi_support::supports_ansi()` once in `set_raw_mode`. To
  check in a checkout on Windows: `srt run` in a project, type at the prompt in Windows Terminal
  and in a plain conhost window, Tab and Up must work, Ctrl-C must end the
  server with the record dropped, and the terminal must be usable
  afterwards.
- **Android.** A tty exists only when flux is launched from a terminal
  emulator (Termux-style) with a keyboard; crossterm's Unix backend uses
  `libc` termios, which Bionic implements. Run `flux` on a script that calls
  `setRawMode(true)` and prints `on("key")` events; confirm keys arrive and
  the terminal is restored on exit. A GUI launch has no stdin and must
  report `isTTY === false`, which the existing path already handles.

Done when both have been run once and any fix is in; a platform that turns
out unusable gets its own item.
