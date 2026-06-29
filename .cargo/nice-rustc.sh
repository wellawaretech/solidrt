#!/bin/sh
# Run rustc (and the linker it spawns) at low CPU + idle I/O priority so a
# full-throttle build still yields to the desktop. Wired in via
# build.rustc-wrapper in .cargo/config.toml; applies to any `cargo`/`make`
# invocation in this workspace regardless of cwd. cargo passes the real rustc
# path as the first argument.
exec nice -n 19 ionice -c 3 "$@"