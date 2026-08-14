---
title: stdin/tty support in flux
description: A flux:stdin (or flux:tty) module for cross-platform raw-mode keystroke reading, the missing piece for any interactive terminal UI under flux, not just the CLI repl.
tags: [flux, tty, stdin, terminal]
created: 2026-07-13
---

# stdin/tty support in flux

Checked directly - there is nothing to reuse anywhere in flux or alloy today:

- `flux:process` (`flux/src/plugins/modules/process.rs`) exposes
  argv/platform/arch/memoryUsage/OS signals. No stdin.
- The `flux`/`fluxc` binaries read stdin, but only as one-shot blocking
  `read_to_string` (piping a whole script in), not interactive raw-mode
  reading.
- `flux:subprocess`'s "stdin" is for writing to a *spawned child's* stdin,
  unrelated to reading our own process's terminal.
- `alloy/src/sdl_utils.rs`'s only stdin hit is `SDL_free` from the C
  `stdinc.h` header - unrelated false positive. SDL keyboard input is
  window/GUI-event based and irrelevant to a headless process anyway.

# Scope

1. **A new `flux:stdin` (or `flux:tty`) module, Rust side.** Put the
   terminal into raw/cbreak mode, read keystrokes, restore terminal state on
   exit/panic. Two platform backends: POSIX termios (Linux/macOS, and Android
   when a real tty/pty is attached - Bionic libc implements the same termios
   ioctls, and this is exactly how Termux-style terminal emulators work
   today) and the Windows Console API (the project ships `win32-x64-msvc`,
   so this isn't skippable). Bridge into the engine's async loop the same
   way `subprocess`'s stdout streaming or `process`'s signal listeners
   already do - keep-alive + emit events, an established pattern in this
   codebase, not a novel one.
2. **JS-side line editing** on top of raw keystroke events: buffer + cursor
   tracking, backspace/arrow keys/enter/ctrl-c/ctrl-d, prompt redraw, a
   history array for up/down. Roughly 150-300 lines, well-understood
   problem, no exotic design needed.
3. **Scoped by "does a tty get attached," not by OS.** Not desktop-only:
   the qualifier is whether the process has a real terminal attached at
   launch, not which platform it's on. That's normally true on
   Linux/macOS/Windows dev machines, and also true on Android when flux is
   invoked from a terminal emulator with a keyboard attached. It's false on
   Android (or desktop) when flux runs as a plain GUI app with no attached
   terminal (`SDL_main` launch, no stdin) - that's a "no tty present" case,
   not an Android-specific exclusion. Worth verifying the termios path
   actually works end-to-end on Android before relying on it, but no reason
   to assume it won't.

# Why it's its own item

This capability is useful on its own, independent of any particular
consumer: anything that wants an interactive terminal UI under flux (a REPL,
a TUI, a debugger prompt) needs it. It should not be scoped or blocked by
whichever project happens to need it first.

# Known consumer

[CLI/flux migration](cli-flux-migration.md) needs this to port
`packages/cli/src/repl.ts`'s `node:readline`-based prompt
(`startRepl` in `repl.ts`, the `createInterface` block) off Bun. That file's actual requirements are narrower
than "full readline": canonical line editing, history, the existing
`completer()` callback (already pure JS, ports unchanged), and a close
event - nothing beyond what's scoped above.

# Suggested starting point

Spike just the raw-mode reader (Rust side, POSIX termios first) in
isolation to validate the approach - bridging it into the async engine loop
and proving keystrokes arrive as expected - before committing to the JS-side
line editor or any consuming project.

# Crate recommendation

Use [`crossterm`](https://github.com/crossterm-rs/crossterm) rather than
hand-rolling termios/Windows Console API calls directly - it's a safe
binding crate, not self-authored unsafe FFI. Cross-platform (Windows down to
7, all Unix
terminals), pure-Rust, minimal deps (`libc` on Unix, `winapi`/`windows-sys`
on Windows), raw mode support, and an async event stream via its
`event-stream` feature (`crossterm::event::EventStream` implements
`futures::Stream`) - bridges directly into flux's existing tokio-based async
loop, the same pattern already used for `subprocess` stdout streaming and
`process` signal listeners. Foundation under `ratatui`/`broot`/`cursive`;
0.29.0, 73M+ downloads, actively maintained as of this writing.

No explicit Android support claimed in its docs, but its Unix backend goes
through `libc`, and `libc`-based crates generally compile for Android
targets (Bionic implements the relevant termios ioctls) - worth a
verification spike rather than an assumption, same caveat as the "tty
attached" scoping above.

Deliberately not `rustyline`: also cross-platform, but it owns the whole
blocking line-read loop internally, which fights this project's thin-FFI
layering (`CLAUDE.md`: plugins marshal, domain logic stays in the owning
module) - the completion/editing behavior belongs in JS, not baked into a
Rust readline implementation. `crossterm`'s raw keystroke-event stream is
the better-fitting primitive: Rust marshals events out, JS does the line
editing.