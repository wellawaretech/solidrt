---
type: backlog-item
title: App icons
description: Stage 1 done (SVG icon from package.json/convention through the manifest to the launcher); the SDL window icon and packed executables remain.
status: partial
timestamp: 2026-07-26T00:00:00Z
---

# App icons

Apps have no icon concept at all today: nothing in `srt pack`, the pack
folder writer, the manifest, or the launcher knows what an app looks like.
The launcher's app list is text rows, and a packed executable takes whatever
the OS gives an unadorned binary.

Raised 2026-07-26 while designing the website's Core concepts. The
"how an app is put together" page needs an asset that is structurally part
of an app rather than invented for the page, and an icon is the obvious one:
every real app has one, it lives in `assets/`, and it demonstrates the
shipped/read-only half of storage without dragging textures onto page one.
Worth doing before that page is written, so the docs describe something real.

## Shape

Follow the packaged-fonts pattern exactly, since it already solves this
problem: the project declares it in `package.json` under `solidrt`, the
packer validates it lives under `assets/`, hashes it into the manifest's
asset list, and adds an identity-level reference next to `displayName`.

```json
{ "solidrt": { "icon": "./assets/icon.svg" } }
```

A convention default (`assets/icon.*` picked up with no declaration) is
worth considering, matching how `assets/` itself is already a convention
the tooling knows.

## Staging

1. **Declare and carry, launcher shows it.** package.json field ->
   validation -> manifest -> `srt:apps` -> the launcher's app list and
   detail view. Self-contained, no native work, and it is the stage the
   docs need.
2. **Desktop window icon.** The running app's window and taskbar entry.
   Needs `SDL_SetWindowIcon`, which the sdl3 crate does not appear to
   expose, so it wants a wrapper in `alloy/src/sdl_utils.rs` per the
   project's SDL rule. Also needs a rasterized surface, so an SVG source
   has to be rendered at a fixed size first.
3. **Packed executables.** Embedding into the binary itself: a Windows
   resource, a macOS bundle icon. Platform-specific and heavier; the
   payoff is the OS file browser showing the icon before the app runs.

Android is a non-issue for stages 2 and 3: apps run inside the client, so
the APK's own icon is the client's, and per-app icons there only ever
affect the launcher list from stage 1.

## Status

Stage 1 landed 2026-07-27: `solidrt.icon` (plus the `assets/icon.svg`
convention default) -> validated in `collectAssets` -> `icon` field in both
dev and pack manifests -> `InstalledApp.icon` carries the SVG source (128 KB
cap, cosmetic failures degrade to absent) -> launcher list and detail render
it via `AppIcon`, with a monogram fallback (muted rounded square + first
letter). The scaffold ships a placeholder `assets/icon.svg`; hello-world has
one as the living example.

Stage 2 landed 2026-07-27, dev-client-only by decision: the production
runtime stays raster-free, so `resvg` is a go-feature dep
(`lattice/src/go/icon.rs` rasterizes at 128, straight-alpha) feeding
`AlloyCommand::SetIcon` -> `sdl_utils::set_window_icon` (SDL_SetWindowIcon
wrapper). The icon follows the app the way the sandbox and fonts do (one call
next to EmitInitEvents per engine spin); the launcher and icon-less apps get
the embedded puzzle mark. Packed runners are deliberately NOT wired: stage 3
platform packaging owns their icons (a Windows .exe resource icon covers
window + taskbar natively; macOS ignores SDL window icons entirely, only the
bundle works). Known gap: packed Linux apps have no window icon until stage 3
decides on .desktop emission.

Stage 3 remains.

## Open question: SVG or PNG

SVG is one file, scales to every surface, and we already render it
(usvg -> d-path), which makes the launcher stage trivial. But the window
icon and any OS-level embedding need rasters at specific sizes, so an SVG
source implies a rasterization step at pack time. A PNG (or a small set)
avoids that and loses the scaling. The launcher's own mark exists in both
forms already (`lattice/assets/icon-puzzle.svg`,
`icon-puzzle-gradient.png`), so either is testable immediately.
