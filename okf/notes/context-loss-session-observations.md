---
title: Context-loss session observations
description: What the 2026-10-15 GPU context-loss work ran into beside its own scope - box state, engine edges seen on the way, tooling traps, and improvements worth a decision - so the next session does not rediscover them.
created: 2026-10-15
---

# Context-loss session observations

Beside the item itself (okf/done/gpu-context-loss.md). None of this was
acted on beyond what is stated; each line says what was seen and what it
might want.

## The Windows box

- Its C: drive was at 99% (6.9 GB free) when the client build started. Three
  stale cargo target dirs from the July linker experiments
  (`lattice/target/lldtest`, `lldtest2`, `reftest`, 10 GB) were deleted,
  which left 17 GB. Still there and candidates for the same treatment:
  `target/debug` (8.8 GB, last written 2026-08-28), `lattice/target/runtime`
  (9.6 GB) and `target/release` (3.6 GB). The client build itself goes to
  `lattice/target/go`.
- The box's git index is still at bcee18e8 (488 commits behind) with the
  working tree overlaid by tar to local HEAD 0aa45a01 plus this session's
  uncommitted changes, vendor submodules copied without `.git`. A
  `git push winbox` keeps being rejected until that is reconciled on the
  box; the memory note on the dev flow says how.
- `~/winbuild`'s cargo passthrough joins its arguments with spaces and then
  splits them again, so a quoted feature list (`--features "go, lattice/test"`)
  breaks into two arguments. Quoting `"$@"` in the script would fix it; the
  comma form works meanwhile.
- A WSL `export` never reaches the Windows process (no WSLENV), and cmd's
  `exe & echo %errorlevel%` prints the errorlevel from before the exe ran.
  Both cost a diagnosis round each; in the memory notes now.

## Engine edges seen on the way

- A lost context is only noticed when a frame is produced: an idle app
  (demand gate closed) stays black until something requests a frame. On a
  real device removal Windows invalidates the window, which should reach
  the liveness latch as an expose; the forced-loss path has no such kick.
  Not measured on an idle app.
- `/settle` answered "settled" immediately while the raster thread was in
  the middle of a recovery (frames refused until the resync), and the
  snapshot that followed got a 502/504. Settle could count a pending resync
  as "in flight"; the dev tools would then wait instead of failing.
- `Context::destroy_texture` is deferred until the render tree no longer
  references the id, so an app cannot stop a flush-rendered target before
  the next frame's flush: the probe's hang target was re-rendered once more
  on the recovered context. The design question is in ideas.md.
- `GpuTexture::allocate_like` gives a compressed 2D texture immutable
  storage (glTexStorage2D). A later id-stable `resize_texture` on such an
  id would respecify immutable storage and fail with a GL error; check
  whether the UI side already rejects resizes of compressed ids (it rejects
  uploads), and make it so if not.
- The recovery rebuilds every buffer from a zero-filled `Vec` of its size,
  so a scene with hundreds of MB of vertex data spikes CPU memory by that
  much for the duration of the rebuild. glBufferData with a null pointer
  would allocate without the copy (`GpuBuffer::new` takes a slice today).
- ANGLE's own `ERR:` lines go straight to stderr, not through the logger,
  and interleave mid-line with ours in a captured log
  (`[alloy] partial repaint off: neither EGL_EXT_bufferERR: Renderer11...`).
  Cosmetic, but it misleads a grep.
- The run loop could end the process without a word (a dropped event
  receiver, a gone raster thread): three info lines were added at those
  exits, plus one where the UI thread's producer returns. They stay.
- forge warns about dead code in `forge/src/audio/trim.rs` (`Trim`, `new`,
  `apply` unused) on every build; pre-existing.

## Coverage gaps

- The recovery ran only on Windows. macOS (the other window-recreation
  platform, ANGLE-Metal) and the Android emulator (same-window context path,
  ANGLE) are in tiny.md. Real Android devices and Mesa have no way to force
  a loss (`/gpu?lose=1` answers `forced: false`), so the same-window path is
  untested anywhere today.
- Nothing in CI exercises it. A Linux lane running the client on ANGLE
  (SwiftShader or ANGLE-over-Vulkan from the packed GL libs) would make
  `/gpu?lose=1` plus the control-API reads a plain app test; that is the one
  improvement that would keep this from rotting.
- The raster-side rebuild (`recover.rs`) has no unit test: it needs a live
  GL context. The headless EGL pbuffer lane could host one (create, lose by
  destroying the context, rebuild on a second one) if a Mesa-side loss can
  be simulated; not obvious how.

## Small things

- `sol run` always opens a local client on this machine's display, even
  when the client under test is remote; a window appeared on the laptop for
  the session's duration. A `--no-client` would spare that.
- `probes/context-loss-probe.tsx` first used `backgroundColor` on
  `<window>`, which is not a window prop (warned, ignored); the probe is
  fixed, but the warning text ("Unknown property") could name the element's
  actual background prop if there is one.
- The okf index regeneration picked up `okf/backlog/3d-contact-hardening-shadows.md`,
  an untracked file of yours, and `okf/notes/3d-feature-parity.md` shows a
  one-line change linking it; both predate this session's edits and were
  left alone.
