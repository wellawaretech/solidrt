---
title: srt render should be headless, unscaled and able to choose its output folder
description: Playback mode opened a hidden SDL window on the real display, laid out at the host's display scale, and wrote frames into the data sandbox; closed by the offscreen video driver (hidden-window fallback), a pinned scale-1 playback resize event, and --out/-o.
tags: [render, playback, capture, headless, cli, determinism]
created: 2026-08-05
completed: 2026-08-05
---

# srt render should be headless, unscaled and able to choose its output folder

`srt render` exists to produce reproducible offscreen PNGs: a virtual clock,
lockstep readback, the embedded fonts. Three things undercut that, all found
while building `scripts/changelog-shot` (a repo tool that renders the
changelog to an image, and the first non-test consumer of this mode).

## 1. Layout is scaled by the host display

`app.rs setup` creates a real, hidden SDL window and
`DisplayContext::new_opengl` picks up `SDL_GetWindowDisplayScale` for it.
`--size 900x4000` therefore allocates a 900x4000 buffer but lays out at
900/scale x 4000/scale and paints at `scale`. On a 1.25 desktop the capture
holds 80% of the content it holds on a 1.0 desktop, at 1.25x the glyph size.

So the same app, the same flags and the same commit produce different images
on different machines - the one property this mode is for. It also means the
app cannot compensate: playback drops resize events
(`app.rs apply_main_thread_effects` returns early for playback), so
`displayScale()` reads 1 and `windowSize()` reads 0x0 in JS.

The mode already tries for 1:1 (the `SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY`
hint, a non-resizable hidden window); pinning `platform.set_display_scale(1.0)`
for playback finishes the job. Then `--size` means physical pixels
everywhere, and a layout box maps to output pixels 1:1.

Workaround in the meantime, if an app needs its own laid-out size in output
pixels: report both its content box and a box that fills the window, and take
the ratio against the requested `--size` height. That is what changelog-shot
does; it should be deletable after this.

## 2. Not actually headless

`packages/cli/AGENTS.md` says renders happen "offscreen via EGL", but the
implementation opens an SDL video window and hides it. It therefore needs a
display: no `srt render` in CI, over a plain SSH session, or on a headless
build box.

SDL3's `offscreen` video driver is the cheap route - request it via the
video-driver hint when `mode.is_playback()`, keep the hidden-window path as a
fallback if the driver cannot hand back a working GL ES context. It also
disposes of item 1 for free: an offscreen display has no scale to inherit.

## 3. Frames land somewhere unaddressable

`PlaybackConfig.output_prefix` is hardcoded to `"frame"` in `main.rs`, and the
runtime chdirs into the app's data sandbox before app code runs, so frames
appear in `~/.local/share/SolidRT/go/client0/apps/default/data/` - not the
directory the command ran in, and not anywhere a caller chose. Any script
using this has to know that path, and two apps rendering under the same id
overwrite each other's frames.

Wanted: `--out <dir-or-prefix>` on the runtime, forwarded by the CLI from the
existing `-o/--output` flag (already in the arg table, unused by `render`),
absolutized before it is passed since the runtime chdirs.

While in there, `packages/cli/AGENTS.md` needs two corrections: the frame
directory it names is one the runtime has not used since storage sandboxing
landed, and the "recording includes a debug overlay" gotcha is stale - the
overlay is gated on `--stats`, which `render` never passes.

## Order

3 then 1 then 2, smallest first: `--out` is a few lines each side, the scale
pin is one line plus a decision about what a playback `Resize` event should
report, and the offscreen driver is the only one that can fail on a given
box and need a fallback path. All three are one `solidrt-go` rebuild.

## Resolution (2026-08-05)

All three landed, in that order:

3. `--out <dir-or-prefix>` on the runtime (absolutized in `main.rs` before
   the sandbox chdir; a directory gets `frame-NNNNNN.png` inside it, anything
   else is a prefix). The CLI always forwards it from `-o/--output`,
   defaulting to the invoking directory, so `srt render` now writes where it
   was run.
1. Playback's one `Resize` event (`playback_resize_event`) reports
   `size_in_pixels` at `display_scale: 1.0` with a full-window safe area, so
   layout units are output pixels and `--size` means physical pixels
   everywhere. Verified byte-identical frames between the offscreen driver
   and a hidden window on a 1.33-scaled Wayland desktop.
2. Playback sets the `SDL_VIDEO_DRIVER=offscreen` hint before video init
   (EGL pbuffer, no display needed; verified with DISPLAY and
   WAYLAND_DISPLAY unset); if the driver cannot produce a GL ES context the
   hint is cleared and setup re-enters the interactive path's hidden window.
   An SDL_VIDEO_DRIVER environment variable still overrides the hint, which
   is how the fallback path can be forced for testing.

The changelog-shot ratio workaround is deleted; the script now reads one
`shot-height` number and collects frames via `-o` from a temp dir. JS-side
`windowSize()` still reads 0x0 in playback (init events are re-emitted via
an AlloyCommand only the interactive loop handles) - separate gap, not
needed for capture sizing now that layout units are output pixels.
