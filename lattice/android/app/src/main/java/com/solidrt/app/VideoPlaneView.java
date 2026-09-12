package com.solidrt.app;

import android.app.Activity;
import android.content.Context;
import android.view.Choreographer;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;

import java.util.concurrent.CountDownLatch;

// The video plane: a SurfaceView a hardware decoder renders straight into,
// composited by SurfaceFlinger beneath SDL's (translucent) surface. It sizes
// itself to the video's aspect inside its parent - letterboxed ("contain") or
// cropping ("cover") - so the compositor's scale does the fitting and no
// pixel of the video ever passes through our frame loop. While attached it
// also samples the display's vsync phase for native: every Choreographer
// frame time crosses to the decoder worker, which snaps its release times
// onto the vsync grid (a desired present time landing just past a latch
// deadline slips a whole period). See okf/plans/android-video-punch-through.md.
public class VideoPlaneView extends SurfaceView implements SurfaceHolder.Callback {
    public static final int FIT_CONTAIN = 0;
    public static final int FIT_COVER = 1;

    private final int videoWidth;
    private final int videoHeight;
    private final int fit;
    // Released once the surface exists (native blocks on it at creation).
    final CountDownLatch created = new CountDownLatch(1);
    // Set by the owner before it removes the view, so a destroy the owner
    // asked for is not reported as a loss.
    volatile boolean releasing;
    // How far after a true hardware vsync the platform wakes this app for
    // it (Display.getAppVsyncOffsetNanos). A Choreographer frame time is
    // that wake-up time, not the vsync, so the grid derived from raw frame
    // times sits this much late (1 ms on a Samsung SM-T500, measured as
    // release requests landing 0.74 of a period before their present where
    // the code asks for 0.80). Read at attach; 0 until then.
    private long appVsyncOffsetNs;

    // One vsync sample per display frame while the view is attached; the
    // UI thread does nothing else while a plane plays, and the fresh phase
    // keeps native's extrapolation (a few periods at most) drift-free. The
    // sample handed on is a true vsync: the frame time less the app's
    // wake-up offset (see appVsyncOffsetNs).
    private final Choreographer.FrameCallback vsync = new Choreographer.FrameCallback() {
        @Override
        public void doFrame(long frameTimeNanos) {
            if (releasing || !isAttachedToWindow()) return;
            SolidRTActivity.nativeVideoPlaneVsync(frameTimeNanos - appVsyncOffsetNs);
            Choreographer.getInstance().postFrameCallback(this);
        }
    };

    VideoPlaneView(Context context, int videoWidth, int videoHeight, int fit) {
        super(context);
        this.videoWidth = Math.max(1, videoWidth);
        this.videoHeight = Math.max(1, videoHeight);
        this.fit = fit;
        // The compositor takes the decoder's buffers at their own size and
        // scales them to the view; the format is the codec's business.
        getHolder().addCallback(this);
    }

    // Fit the video's aspect into the space the parent offers; the parent
    // centers the result (RelativeLayout CENTER_IN_PARENT).
    @Override
    protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
        int boxW = MeasureSpec.getSize(widthMeasureSpec);
        int boxH = MeasureSpec.getSize(heightMeasureSpec);
        if (boxW <= 0 || boxH <= 0) {
            setMeasuredDimension(boxW, boxH);
            return;
        }
        double scaleW = (double) boxW / videoWidth;
        double scaleH = (double) boxH / videoHeight;
        double scale = fit == FIT_COVER ? Math.max(scaleW, scaleH) : Math.min(scaleW, scaleH);
        int w = (int) Math.round(videoWidth * scale);
        int h = (int) Math.round(videoHeight * scale);
        setMeasuredDimension(w, h);
    }

    // The surface a decoder is configured with (valid once `created` fired).
    public Surface surface() {
        return getHolder().getSurface();
    }

    // The display's refresh period in nanoseconds (the vsync grid's spacing;
    // its phase comes from the Choreographer samples). 0 when unknown.
    public long refreshPeriodNs() {
        float rate = ((Activity) getContext()).getWindowManager().getDefaultDisplay().getRefreshRate();
        return rate > 0 ? Math.round(1e9 / rate) : 0;
    }

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        appVsyncOffsetNs = ((Activity) getContext()).getWindowManager().getDefaultDisplay().getAppVsyncOffsetNanos();
        Choreographer.getInstance().postFrameCallback(vsync);
    }

    @Override
    protected void onDetachedFromWindow() {
        Choreographer.getInstance().removeFrameCallback(vsync);
        super.onDetachedFromWindow();
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        created.countDown();
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
    }

    // The system took the surface (the activity went to the background). The
    // codec holding it must stop; native learns it here and ends playback.
    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        if (!releasing) {
            SolidRTActivity.nativeVideoPlaneLost();
        }
    }
}
