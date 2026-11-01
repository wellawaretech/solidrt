---
title: Hold Android window draws until the engine has drawn into its surface
description: On every Android cold start the launcher flashes between the system splash and the app's first frame, because SDL's translucent SurfaceView opens its hole in the window before native has queued a buffer; implement SurfaceHolder.Callback2 so every window draw that needs a new SDL frame waits for one, which also keeps the splash icon up until the first frame.
created: 2026-11-01
completed: 2026-11-01
---

# Hold Android window draws until the engine has drawn into its surface

What it looks like: launching a packed app (`sol pack --apk`) on Android
12+ shows the system splash (icon on a dark background), then the launcher
and wallpaper for a moment, then the app's first frame. Every cold start.
Reported from the tiles demo.

## Cause

Three facts combine, each checked against our code and the Android 14
framework source:

1. SDL's surface is translucent and the window is see-through over it.
   `SolidRTActivity.createSDLSurface` sets `PixelFormat.TRANSLUCENT` and
   `setZOrderMediaOverlay(true)` for the video plane
   (okf/done/android-video-punch-through.md). The surface sits beneath the
   activity window. A view tree with a SurfaceView requests a transparent
   region, and `ViewRootImpl` then forces the window format to translucent.
   Once the SurfaceView has "finished drawing" (`mDrawFinished`), its
   `draw()` clears its rectangle in the window to transparent.
2. The splash leaves when the window reports its first frame drawn. That
   frame comes at once from the Java view tree. It still shows the theme's
   black `windowBackground` over the SDL area, because the hole is punched
   only after `mDrawFinished`.
3. The hole opens before native has drawn anything. `SDLSurface` is a plain
   `SurfaceHolder.Callback`, so `SurfaceCallbackHelper` finishes the
   redraw at once and the SurfaceView punches its hole on the next window
   draw. SDL starts `SDL_main` only now (`handleNativeState` waits for
   `mIsSurfaceReady`). So for the whole native boot (SDL_main, the engine,
   the GL context) the hole is open over a surface with no buffer at all.
   Beneath the activity window during the opening transition is the
   launcher.

After that, `prime_window` (alloy/src/raster/frame.rs) swaps an opaque
black clear, and the first UI frame follows: a transparent clear plus the
app's paint.

The comment in `createSDLSurface` is wrong: uncovered pixels do not show
"the window's black background". They show what is beneath the activity
window. That is nothing once a transition ends (the launcher is stopped,
so it reads black) and the launcher or recents during one.

## Fix: SolidRTSurface implements SurfaceHolder.Callback2

Point 3 is the root: the SurfaceView declares its redraw finished before
anything was drawn. `Callback2.surfaceRedrawNeededAsync` is the platform's
way to say "not yet": SurfaceView then keeps the hole closed, and the
window's draw report waits until the callback's `drawingFinished` runs.
NativeActivity waits for its app the same way. SDL registers the surface
object itself as the holder callback, so the subclass implementing
`Callback2` is enough.

This covers every surface, not only the first:

- Cold start: the window's first draw report waits for the engine's first
  frame, so the splash icon stays up through the native boot and leaves
  over the app's first frame.
- Return from background: the recreated surface gets a frame before the
  hole opens over it.
- Rotation and resize: the window's frame waits for our frame at the new
  size, so no mis-sized frame shows in between.

### Java

In `SolidRTSurface`, sketched:

```java
class SolidRTSurface extends SDLSurface implements SurfaceHolder.Callback2 {
    // Bounds a wedged engine: a window draw never waits longer than this.
    private static final long REDRAW_HOLD_MAX_MS = 5000;

    // The redraw SurfaceView waits for. UI thread only.
    private Runnable pendingRedraw;

    // Tells native that the next frame it begins must be reported back.
    private static native void nativeRedrawNeeded();

    // Required by Callback2, never reached: SurfaceView calls the async
    // variant on API 26+ (our minSdk).
    @Override
    public void surfaceRedrawNeeded(SurfaceHolder holder) {
    }

    @Override
    public void surfaceRedrawNeededAsync(SurfaceHolder holder, Runnable drawingFinished) {
        // A newer request supersedes an older one. Finish the older, or
        // its sync never completes.
        finishRedraw();
        pendingRedraw = drawingFinished;
        nativeRedrawNeeded();
        postDelayed(() -> {
            if (pendingRedraw == drawingFinished) finishRedraw();
        }, REDRAW_HOLD_MAX_MS);
    }

    void finishRedraw() {
        Runnable finished = pendingRedraw;
        pendingRedraw = null;
        if (finished != null) finished.run();
    }
}
```

Native reports back through a static on `SolidRTActivity`, which can
reach `mSurface` (a protected static of SDLActivity). It posts
`((SolidRTSurface) mSurface).finishRedraw()` to the UI thread.

### Native

- `nativeRedrawNeeded` goes next to `nativeTouch` in lattice/src/lib.rs
  and forwards to alloy. Alloy sets a static flag, like `PLANE_LOST` in
  alloy/src/video_plane.rs: at cold start the request arrives before the
  loop exists. Then it wakes the loop with a repaint. `AlloyEvent::Exposed`
  is the existing repaint trigger with no JS side (alloy/src/liveness.rs).
  The repaint is needed because Android also asks for a redraw when
  nothing in the app changed. A demand-driven engine at rest would draw
  nothing, and the window would wait for the timeout.
- Release on the present of the first frame begun after the request: the
  loop takes the flag when it starts a frame and reports on that frame's
  `FrameOutput::Presented` (alloy/src/app.rs). A frame already in flight
  when the request came may hold the old size or target the old surface,
  so it does not count. `prime_window` is not a frame, so its black clear
  never releases a hold.
- The report is one JNI call through `sdl_utils::with_android_activity`,
  the shared helper the dialogs and the video plane already use. Built
  that way: the ask and the answer live in alloy/src/redraw.rs, the
  decision in liveness.rs.

### Checked, no deadlock

- SDL starts its thread without window focus on API 24+
  (`mHasMultiWindow`), and our minSdk is 26. So the held window draw does
  not stop the engine from starting.
- The engine gets its surface before the hold starts: `surfaceCreated`
  and `surfaceChanged` (which starts `SDL_main`) run before the redraw
  callback, in the same `updateSurface`.
- Below API 31 there is no system splash. The legacy starting window (the
  theme's black `windowBackground`) is held the same way, until the first
  frame.

### Risks

- That the splash waits for the async redraw is read from the framework
  source (the window's draw report waits on the SurfaceView's redraw), not
  from documentation. The documented API 31+ way to keep the splash is an
  `OnPreDrawListener` on the content view that cancels drawing until ready.
  If the recording shows the splash leaving early, add that listener for
  the first draw, released by the same native report. A cancelled draw
  reschedules itself every frame, so the listener needs no wake-ups.
- An engine stall now holds the system's window transitions (return,
  rotation) until the timeout. Today it freezes only our own surface.

## Not in scope

The hold ends on the first presented frame, not when the app is ready. For
the tiles demo the first frame is the menu: index.tsx renders the
background and the `/` route synchronously. Only the best-score lines
arrive later, from an async SQLite read. An app whose first frame is a
loading screen drops the splash on that frame. An app-ready signal (like
Android's `setKeepOnScreenCondition`) is a separate API decision.

## Done looks like

- Cold start recorded with `adb shell screenrecord` and stepped frame by
  frame, with a bright wallpaper. On an API 31+ device (the SM-T500
  tablet), the icon stays up until the first frame and no launcher frame
  shows. On a pre-31 device, black until the first frame.
- Return from background and a rotation recorded: no launcher, recents or
  mis-sized frame.
- An app at rest still releases at once on a redraw request with no change
  in it, for example a return to a static screen. This tests the repaint
  path.
- With the native report disabled, the timeout releases every hold.
- The `createSDLSurface` comment says what is beneath: the activity
  window's underside, not its background.

## Related

Colouring the splash from the project's `iconBackground` is its own item:
[[android-splash-colour]].

## Outcome

Built as shaped. Two deviations: `with_android_activity` (sdl_utils.rs) was
already the shared JNI helper, so nothing moved; and it leaked JNI local
references on every call, which the answer path made frequent, so it now
runs its closure in a local frame. Recording method and the measured facts
around this item: [[android-launch-handover]].

Verified 2026-11-01, launches recorded and stepped frame by frame:

- SM-T500 (API 31), Player.dev from the app drawer: before, the icon left
  at 1.51 s and 1.8 s of black followed; after, the icon stays to 3.38 s
  and fades into the first frame, with time to first frame unchanged.
- SM-T500, packed apps: an app packed on the old runner shows 0.56 s of
  black after the icon; one packed on the new runner goes from the icon
  into its first frame with no black and no launcher.
- TPM171E TV (API 26), Player.dev: before, 3.6 s of pure black between the
  starting window and the first frame; after, the starting window stays
  until it crossfades into the first frame.
- Return from background: app content from the first animation frame.
- Rotation: the screen stays frozen until the correct frame (0.7 s to
  1.2 s) instead of unfreezing after 150 ms onto a stale, wrong-size frame;
  the correct frame lands at the same moment either way.
- Timeout, with the answer disabled: the window shows 5.01 s after the
  request, with the warning in logcat.
