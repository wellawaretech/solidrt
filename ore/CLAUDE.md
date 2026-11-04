# Ore

Ore is the OS layer and the platform boundary: every OS facility, one
contract each. It wraps SDL where SDL reaches and our own native code (JNI,
Win32, Objective-C, D-Bus) where it stops; SDL is its implementation
detail. Read okf/design/platform-layering.md before adding a facility: the
layers, the SDL thread rules, the test set of platform features and the
migration plan are there.

# What belongs here
Anything that reaches the OS or SDL: anything per platform, any
`cfg(target_os)`. New work of that kind lands here, never in alloy, forge or
lattice (the root CLAUDE.md, Projects, has the placement order). Existing OS
code in those crates moves here when a feature touches it, not as a sweep.

Ore depends on SDL and the OS libraries only, on nothing of ours. It names
no scripting-engine types: flux `ore_plugins/` marshals an app-facing
facility to JS (a web-standard surface through `standards_plugins/`), and
lattice calls ore for host concerns only.

# Contracts
One contract per facility, the same on every platform. Where a platform
cannot meet part of it, the module doc says so plainly, and availability is
a fact the facility reports (`link::available`), which an embedder lists as
a capability. Platform differences end here: what ore hands upward is
normalized.

A facility a test must take the place of has a session an embedder keeps
per engine (`link::Session`), the same shape as the dialog session: the log a
test or an agent reads back, and a mode in which the platform is never
reached.

# Threads
SDL's thread rules decide how a facility is called (the table in the design
note). "Not thread safe" means one caller at a time: a lock in the module
(`locale.rs`). "Main thread only" is real affinity: the call goes through
the bridge (`main_thread::run`), which runs it inside the main thread's
event wait or at once where SDL has no event loop. Never block a caller on
a main-thread call; answer through a responder the call captures.

# SDL calls
The sdl3 crate first. What it lacks goes through `sdl3::sys` in the module
of its facility (not a shared utils file), every `unsafe` block with a
`SAFETY:` line on what makes it sound. Read SDL's errors through
`sdl_error()`, never `sdl3::get_error()` (it panics on a message that is not
UTF-8).

# Tests
Unit tests live in the single `src/tests/` folder (`src/tests/<module>.rs`),
never inline in source files.
