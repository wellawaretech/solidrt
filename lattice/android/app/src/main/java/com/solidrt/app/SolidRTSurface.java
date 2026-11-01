package com.solidrt.app;

import android.content.Context;
import android.os.Build;
import android.util.Log;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.View;

import org.libsdl.app.SDLSurface;

// SDL's surface with a touch path and held redraws of our own.
//
// Touch: SDL hands native a touch without its time (it is stamped when the
// call arrives) and never reads a batch's historical samples, while Android
// delivers a time with every sample. Finger events therefore go to
// nativeTouch, every sample with its own time, and SDL sees none of them;
// mouse and stylus events stay with SDL.
//
// Held redraws: the surface sits beneath a window that is see-through over
// it (SolidRTActivity.createSDLSurface), so a surface without a frame shows
// what is beneath the app, the launcher during a launch. SDL's plain
// Callback lets SurfaceView declare each redraw finished at once; as a
// Callback2 the surface instead holds it until native has drawn a frame
// into it. Meanwhile SurfaceView keeps the window covering it, and the
// window's draw report waits, so the splash stays up through a launch and
// no stale or mis-sized frame shows on a return or a rotation.
class SolidRTSurface extends SDLSurface implements SurfaceHolder.Callback2 {
    // Bounds a wedged engine: the platform never waits longer than this for
    // a held redraw.
    private static final long REDRAW_HOLD_MAX_MS = 5000;

    // Numbers the held redraws, so a late answer to an older one never
    // releases a newer one. Static: unique across surfaces in the process.
    // UI thread only, as are the two fields below.
    private static int redrawGeneration;
    private int pendingGeneration;
    // SurfaceView's continuation for the held redraw; null when none.
    private Runnable pendingRedraw;

    SolidRTSurface(Context context) {
        super(context);
    }

    // Asks native for a frame built after this call; it answers through
    // SolidRTActivity.redrawPresented with the same generation.
    private static native void nativeRedrawNeeded(int generation);

    // Required by Callback2 but never reached: SurfaceView calls the async
    // variant below on API 26+, our minSdk.
    @Override
    public void surfaceRedrawNeeded(SurfaceHolder holder) {
    }

    @Override
    public void surfaceRedrawNeededAsync(SurfaceHolder holder, Runnable drawingFinished) {
        // A newer redraw supersedes the held one: finish that, or its sync
        // never completes.
        finishRedraw(pendingGeneration);
        final int generation = ++redrawGeneration;
        pendingGeneration = generation;
        pendingRedraw = drawingFinished;
        nativeRedrawNeeded(generation);
        postDelayed(() -> {
            if (finishRedraw(generation)) {
                Log.w(SolidRTActivity.TAG, "Redraw not answered in " + REDRAW_HOLD_MAX_MS + " ms: showing the window anyway");
            }
        }, REDRAW_HOLD_MAX_MS);
    }

    // Releases held redraw `generation`, if it is still the held one. True
    // when it was.
    boolean finishRedraw(int generation) {
        if (pendingRedraw == null || generation != pendingGeneration) {
            return false;
        }
        Runnable finished = pendingRedraw;
        pendingRedraw = null;
        finished.run();
        return true;
    }

    // One finger sample: the pointer's id, the MotionEvent's masked action,
    // the position normalized to the surface (0 to 1, as SDL's touch
    // events have it) and the sample's time in nanoseconds on the
    // SystemClock.uptimeMillis base (CLOCK_MONOTONIC).
    private static native void nativeTouch(int pointerId, int action, float x, float y, long timeNanos);

    @Override
    public boolean onTouch(View v, MotionEvent event) {
        final int count = event.getPointerCount();
        for (int i = 0; i < count; i++) {
            int tool = event.getToolType(i);
            if (tool != MotionEvent.TOOL_TYPE_FINGER && tool != MotionEvent.TOOL_TYPE_UNKNOWN) {
                return super.onTouch(v, event);
            }
        }
        final int action = event.getActionMasked();
        // A pointer going down or up beside others is the one event about
        // that pointer alone; every other action carries all of them.
        if (action == MotionEvent.ACTION_POINTER_DOWN || action == MotionEvent.ACTION_POINTER_UP) {
            int i = event.getActionIndex();
            nativeTouch(event.getPointerId(i), action, normalizedX(event.getX(i)), normalizedY(event.getY(i)), timeNanos(event));
            return true;
        }
        // The samples batched since the last delivery, oldest first, then
        // the current one. Only a move carries any.
        final int history = event.getHistorySize();
        for (int h = 0; h < history; h++) {
            long time = historicalTimeNanos(event, h);
            for (int i = 0; i < count; i++) {
                nativeTouch(event.getPointerId(i), action,
                    normalizedX(event.getHistoricalX(i, h)), normalizedY(event.getHistoricalY(i, h)), time);
            }
        }
        long time = timeNanos(event);
        for (int i = 0; i < count; i++) {
            nativeTouch(event.getPointerId(i), action, normalizedX(event.getX(i)), normalizedY(event.getY(i)), time);
        }
        return true;
    }

    // Sample times are whole milliseconds before Android 14, which added
    // the nanosecond readings.
    private static long timeNanos(MotionEvent event) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            return event.getEventTimeNanos();
        }
        return event.getEventTime() * 1000000L;
    }

    private static long historicalTimeNanos(MotionEvent event, int pos) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            return event.getHistoricalEventTimeNanos(pos);
        }
        return event.getHistoricalEventTime(pos) * 1000000L;
    }

    private float normalizedX(float x) {
        return mWidth <= 1 ? 0.5f : x / (mWidth - 1);
    }

    private float normalizedY(float y) {
        return mHeight <= 1 ? 0.5f : y / (mHeight - 1);
    }
}
