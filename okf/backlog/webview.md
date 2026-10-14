---
title: Embed a webview
description: Apps cannot show web content inside their UI; the first lane is the OS webview as a native plane at an element's rect (Android by the video-plane pattern, Windows and macOS through wry), Linux on Wayland has no plane and needs either "unavailable" or a WPE texture lane, and the use case decides the scope.
created: 2026-10-14
---

# Embed a webview

An app cannot show web content inside its own UI. Which needs this has to
serve is not settled yet, and that decides the scope (see Open questions).

The rule from [android-video-punch-through](../done/android-video-punch-through.md),
"a surface is a clock", splits this the same way it split video: a native
view the system composites over or under our window (a plane), or offscreen
frames we import (a texture). Some needs are better met by not embedding at
all.

## Lane 0: hand off instead of embedding

Login, payment pages and links to documentation: open the system browser,
or Custom Tabs on Android. For OAuth this is the only real option: Google
rejects sign-in from embedded webviews (`disallowed_useragent`), and RFC
8252 (OAuth for native apps) prescribes the system browser.

## Lane 1: plane, the OS webview at an element's rect

The first lane to build.

- **Android.** Copy the `VideoPlaneView` pattern: an `android.webkit.WebView`
  added by `SolidRTActivity`, its JNI handle held in alloy next to
  `video_plane.rs`, created and destroyed by identity (the engine-reload
  trap from the video plane applies unchanged: the old engine is dropped
  after the new one opened its view). The SDL surface is already
  translucent with `setZOrderMediaOverlay`, so a WebView in the window's
  view hierarchy sits beneath it and our UI composites over it through
  the hole, as it does over video.
- **Windows and macOS.** WebView2 as a child of SDL's HWND, WKWebView as a
  subview of the NSWindow's content view; SDL3 hands out both through
  window properties. `wry` (Tauri's crate, maintained, widely used) covers
  both through `build_as_child` with a raw window handle, which fits the
  dependency rule as a safe binding crate. Its Android path is built
  around Tauri's own generated activity glue, so Android keeps our own JNI
  view. On desktop the child view sits above the GL surface: no UI can
  draw over it (no popup, toast or dropdown over the page).
- **Linux.** WebKitGTK needs a GTK widget tree and SDL's window is not one.
  wry embeds a child on X11 only; on Wayland there is no route, since GTK
  and SDL each hold their own Wayland connection and neither can host the
  other's surface. So the plane lane has nothing for the dev machine
  (Hyprland) or a Wayland Pi or kiosk. Taking the X11 route would also
  make the binary depend on WebKitGTK, a large system library, which
  breaks the single-binary footprint.

What the plane gives for free: real browser input, IME, scrolling, text
selection, accessibility and remote devtools.

What it costs:

- An unclipped rectangle only: no transforms, opacity, effects, or
  clipping by a ScrollView ancestor. A rectangular clip through a clipping
  container view is possible later.
- Invisible to `/snapshot` and to headless test mode (video plane trap:
  our capture reads the GL layer, which is a transparent hole there). On
  Android, `adb exec-out screencap` reads the composed screen.
- Focus handoff. On TV, D-pad focus moving between the SDL surface and the
  WebView, and back out of the page, needs its own design next to the
  launcher's remote navigation.

## Lane 2: texture, offscreen frames imported

Later, and only if composition is needed: a page in a ScrollView, under a
popup, on a 3D mesh, in snapshots and headless tests.

- **What it costs.** We forward all input ourselves (pointer, keys, IME,
  wheel, cursor) and lose accessibility.
- **Engines differ per OS.**
  - Linux: WPE WebKit, the embedded WebKit port that ships on set-top
    boxes and hands out frames as dmabuf/EGLImage. The natural fit for TV
    and Pi, and the only answer for Wayland. No established Rust binding:
    a thin FFI module against its C API (dependency rule, option 2).
  - Android: a WebView drawn into a SurfaceTexture, sampled as an external
    OES texture.
  - Desktop: CEF offscreen rendering. Chromium with helper processes and
    well over 100 MB, which breaks the footprint; not a starting point.
- **Prerequisite.** alloy has no zero-copy import of external images
  today (no EGLImage, dmabuf, external OES or shared-handle path). That is
  a separable piece of work, and the camera and video texture paths would
  benefit from it too.

## Shape

- **Optional module.** Behind a lattice feature in the sense of
  [module-feature-flags](module-feature-flags.md), outside the stock
  default set, so a runtime that does not want it carries none of its
  dependencies. Availability by name through `Flux.capabilities`, so an
  app on Linux/Wayland sees "unavailable" rather than an empty rect.
- **Layering.** The native view's lifecycle and placement live in alloy.
  The rendertree only reports the element's laid-out, pixel-snapped window
  rect and stays engine-free; the `alloy_plugins` layer marshals and
  nothing more.
- **JS shape.** `<iframe>` through the SolidRT lens:
  - It takes `src` and is laid out by taffy like any box; the native view
    follows its rect after layout.
  - App to page: the element's `postMessage(data)`. Page to app: an
    `onMessage` prop. The page keeps the standard
    `window.parent.postMessage`, through a small injected script over each
    platform's bridge (WebView2 `chrome.webview.postMessage`, WKWebView
    script message handlers, Android `addJavascriptInterface`; wry exposes
    an IPC handler on desktop).
  - Pages bundled with the app load through a custom scheme rather than
    `file://` (wry custom protocols, `shouldInterceptRequest` on
    Android).

## Done looks like (lane 1)

- The `<iframe>`-shaped host element with `src`, load notification and
  the two-way `postMessage`, bundled pages through the custom scheme.
- Android first (the plane pattern is proven there), verified on the TV
  and the tablet, then Windows and macOS through wry.
- Linux reports the capability as unavailable, unless the answer to the
  second open question is the WPE lane.

## Open questions

1. What do apps need it for: link-out or login (lane 0), a rectangle of web
   content (lane 1), or web content blended into the UI or 3D (lane 2)?
2. Does it have to work on Linux/Wayland (Pi, kiosk, the dev machine)? If
   yes, that is lane 2 with WPE, which needs the zero-copy import first.
3. TV focus handoff between our UI and the page.
