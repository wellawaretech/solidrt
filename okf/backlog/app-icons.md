---
title: App icons
description: Stages 1+2 done (SVG icon from package.json/convention through the manifest to the player, monogram fallback; dev-client window icon via go-gated resvg + SDL_SetWindowIcon); stage 3 packed executables remains and owns packed-app icons on all platforms, including the Android TV banner a packed APK inherits from the runner.
created: 2026-07-27
---

# App icons

Originally: apps had no icon concept at all - nothing in `srt pack`, the
pack folder writer, the manifest, or the player knew what an app looked
like, and a packed executable took whatever the OS gives an unadorned
binary. Stages 1 and 2 below have since shipped (see Status); only the
packed-executable half is still true.

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

1. **Declare and carry, player shows it.** package.json field ->
   validation -> manifest -> `srt:apps` -> the player's app list and
   detail view. Self-contained, no native work, and it is the stage the
   docs need.
2. **Desktop window icon.** The running app's window and taskbar entry.
   Needed `SDL_SetWindowIcon`, which the sdl3 crate does not expose, so it
   got a wrapper in `alloy/src/sdl_utils.rs` per the project's SDL rule,
   plus a rasterization step (resvg) since the source is SVG. Shipped, see
   Status.
3. **Packed executables.** Embedding into the binary itself: a Windows
   resource, a macOS bundle icon. Platform-specific and heavier; the
   payoff is the OS file browser showing the icon before the app runs.

Android, dev client: apps run inside it, so the APK's icon is the client's
and per-app icons only reach the player list (stage 1). A packed APK
([standalone-android-apk](../done/standalone-android-apk.md)) retires that:
its patcher takes a PNG into an adaptive icon (a 22.5% inset foreground
over a solid background), and stage 3 owns what is left there:

- The Android TV banner. A TV launcher shows `android:banner` (160x90dp)
  instead of the icon and label, so a banner without text is an anonymous
  tile. The go client's `res/drawable-xhdpi/tv_banner.png` is mark-only
  and should carry "Player"; a packed app inherits that same static
  banner, so on a TV every packed app shows the SolidRT banner, not its
  own. Wanted: a patchable banner slot (the `app_icon_fg.png` mechanism)
  plus pack-time composition of the app's `displayName` over its icon,
  which shares the text-at-pack-time problem with
  [icon-svg-rasterization](icon-svg-rasterization.md).
- An inset override for adaptive-aware full-bleed art, and a
  `<monochrome>` layer for themed icons, both additive on the patcher.

## Status

Stage 1 landed 2026-07-27: `solidrt.icon` (plus the `assets/icon.svg`
convention default) -> validated in `collectAssets` -> `icon` field in both
dev and pack manifests -> `InstalledApp.icon` carries the SVG source (128 KB
cap, cosmetic failures degrade to absent) -> player list and detail render
it via `AppIcon`, with a monogram fallback (muted rounded square + first
letter). The scaffold ships a placeholder `assets/icon.svg`; hello-world has
one as the living example.

Stage 2 landed 2026-07-27, dev-client-only by decision: the production
runtime stays raster-free, so `resvg` is a go-feature dep
(`lattice/src/go/icon.rs` rasterizes at 128, straight-alpha) feeding
`AlloyCommand::SetIcon` -> `sdl_utils::set_window_icon` (SDL_SetWindowIcon
wrapper). The icon follows the app the way the sandbox and fonts do (one call
next to EmitInitEvents per engine spin); the player and icon-less apps get
the embedded puzzle mark. Packed runners are deliberately NOT wired: stage 3
platform packaging owns their icons (a Windows .exe resource icon covers
window + taskbar natively; macOS ignores SDL window icons entirely, only the
bundle works). Known gap: packed Linux apps have no window icon until stage 3
decides on .desktop emission.

Stage 3 remains.

## Decided: SVG

SVG won for both shipped stages: one file, scales to every surface, already
rendered by the engine (usvg -> d-path) for the player, and rasterized at
a fixed size by resvg for the window icon. The default/fallback mark is the
embedded `lattice/assets/icon-puzzle-gradient.svg`. Stage 3's OS-level
embedding will still need fixed-size rasters produced at pack time.
