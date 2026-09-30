package com.solidrt.app;

import android.content.Context;
import android.os.Build;
import android.view.MotionEvent;
import android.view.View;

import org.libsdl.app.SDLSurface;

// SDL's surface with a touch path of our own. SDL hands native a touch
// without its time (it is stamped when the call arrives) and never reads a
// batch's historical samples, while Android delivers a time with every
// sample. Finger events therefore go to nativeTouch, every sample with its
// own time, and SDL sees none of them; mouse and stylus events stay with
// SDL.
class SolidRTSurface extends SDLSurface {
    SolidRTSurface(Context context) {
        super(context);
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
