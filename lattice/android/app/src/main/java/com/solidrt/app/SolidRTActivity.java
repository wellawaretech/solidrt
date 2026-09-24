package com.solidrt.app;

import android.content.Context;
import android.content.Intent;
import android.graphics.PixelFormat;
import android.hardware.display.DisplayManager;
import android.hardware.input.InputManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.Display;
import android.view.InputDevice;
import android.view.View;
import android.view.WindowInsets;
import android.widget.RelativeLayout;

import java.util.ArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import org.libsdl.app.SDLActivity;
import org.libsdl.app.SDLSurface;

// The SolidRT activity body shared by both flavors: the go dev client and the
// production runtime (each flavor's MainActivity subclasses this; the
// manifest component name com.solidrt.app.MainActivity is the launch
// contract). Owns the native library set and the keyboard facts the runtime
// cannot observe from SDL; flavor-specific behavior (the go client's asset
// extraction and dev-server intent extra) lives in the subclasses.
public class SolidRTActivity extends SDLActivity {
    protected static final String TAG = "SolidRT";

    // Whether this activity is being recreated from saved state: the system
    // ended the previous instance on its own (a background kill) and is
    // restoring the task, as opposed to a launch the user started (first
    // start, or after exit()/close finished the activity). Captured in
    // onCreate; getArguments runs later, on the SDL thread.
    private boolean restored;

    // The link this launch carries (a VIEW intent's data, as the OS routed a
    // registered scheme here), raw; null for a plain launch. Read in onCreate
    // and cleared from the intent before SDL sees it: SDLActivity would
    // forward only its path as a drop file, losing scheme and host.
    private String launchLink;

    @Override
    protected String[] getLibraries() {
        return new String[] {
            "SDL3",
            "impeller",
            "main"
        };
    }

    // Reports that the system destroyed the video plane's surface (see
    // VideoPlaneView): the decoder rendering into it has to stop.
    static native void nativeVideoPlaneLost();

    // Forwards one Choreographer frame time (System.nanoTime base) while a
    // video plane is attached: the phase of the display's vsync grid.
    static native void nativeVideoPlaneVsync(long frameTimeNanos);

    // The video plane (okf/plans/android-video-punch-through.md): at most one
    // SurfaceView beneath SDL's, created and removed on request from native.
    private VideoPlaneView videoPlane;

    // Milliseconds to wait for the plane's surface to come up on the UI
    // thread; a view added to a resumed window gets its surface within a
    // frame or two, so this only bounds a wedged UI thread.
    private static final long VIDEO_PLANE_CREATE_TIMEOUT_MS = 2000;

    // SDL's surface sits above the video plane and lets it show through: the
    // media-overlay z-order puts it over other SurfaceViews (both stay beneath
    // the activity window), and the translucent format is what makes
    // SurfaceFlinger blend it instead of treating the layer as opaque
    // whatever its buffers hold. The GL side asks for alpha bits to match
    // (alloy configure_opengl) and already clears the backbuffer to
    // transparent black, so uncovered pixels show what is beneath: the video
    // plane while one exists, else the window's black background as before.
    // Both settings must precede the window attaching, which is why they are
    // made at surface creation rather than in onCreate.
    @Override
    protected SDLSurface createSDLSurface(Context context) {
        SDLSurface surface = super.createSDLSurface(context);
        surface.getHolder().setFormat(PixelFormat.TRANSLUCENT);
        surface.setZOrderMediaOverlay(true);
        return surface;
    }

    // Creates the video plane sized to fit a videoWidth x videoHeight picture
    // (fit: VideoPlaneView.FIT_*) and returns the view once its surface
    // exists (VideoPlaneView.surface() is what the decoder takes). Called
    // from native on a non-UI thread; the view work runs on the UI thread and
    // this blocks until the surface is created. Null when a plane already
    // exists (one at a time) or the surface did not come up.
    //
    // The view is the handle, and destroyVideoPlane takes it back by
    // identity rather than "the current plane": a reload tears the old
    // engine down after the new one has opened its plane, so the posted
    // removal of the old view can run after the new view was added.
    public VideoPlaneView createVideoPlane(final int videoWidth, final int videoHeight, final int fit) {
        final VideoPlaneView[] made = new VideoPlaneView[1];
        final CountDownLatch added = new CountDownLatch(1);
        runOnUiThread(() -> {
            try {
                if (videoPlane != null) return;
                VideoPlaneView view = new VideoPlaneView(this, videoWidth, videoHeight, fit);
                RelativeLayout.LayoutParams params = new RelativeLayout.LayoutParams(
                    RelativeLayout.LayoutParams.MATCH_PARENT, RelativeLayout.LayoutParams.MATCH_PARENT);
                params.addRule(RelativeLayout.CENTER_IN_PARENT);
                // Index 0: beneath SDL's surface in the view order as well.
                mLayout.addView(view, 0, params);
                videoPlane = view;
                made[0] = view;
                Log.v(TAG, "Video plane added (" + videoWidth + "x" + videoHeight + ", fit " + fit + ")");
            } finally {
                added.countDown();
            }
        });
        try {
            if (!added.await(VIDEO_PLANE_CREATE_TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
                Log.w(TAG, "Video plane not created: UI thread stalled");
                return null;
            }
            VideoPlaneView view = made[0];
            if (view == null) {
                Log.w(TAG, "Video plane not created: one already exists");
                return null;
            }
            if (!view.created.await(VIDEO_PLANE_CREATE_TIMEOUT_MS, TimeUnit.MILLISECONDS)) {
                Log.w(TAG, "Video plane surface did not come up");
                destroyVideoPlane(view);
                return null;
            }
            return view;
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return null;
        }
    }

    // Removes the given video plane. Native calls this after the decoder
    // released the surface; a view already removed is a no-op.
    public void destroyVideoPlane(final VideoPlaneView view) {
        runOnUiThread(() -> {
            view.releasing = true;
            if (videoPlane == view) videoPlane = null;
            mLayout.removeView(view);
            Log.v(TAG, "Video plane removed");
        });
    }

    // Forwards the soft keyboard (IME) inset height in pixels to native. The
    // window is fullscreen/edge-to-edge, so the OS will not resize for the
    // keyboard; the app lifts its own content using this value instead.
    private static native void nativeKeyboardInset(int px);

    // Forwards hardware-keyboard presence to native. SDL's Android backend
    // does not track keyboards, so native cannot see this itself; the runtime
    // uses it to keep the on-screen keyboard down while one is attached.
    private static native void nativeHardwareKeyboard(boolean present);

    // A hardware keyboard the user can actually type on. Neither
    // Configuration.keyboard nor a bare InputDevice keyboard-type check is
    // trustworthy on TVs: built-in remote/driver devices claim an alphabetic
    // QWERTY keyboard (seen live: Philips TPV_LKB/TPV_MutilRC and MediaTek
    // mtkinp_events, all KEYBOARD_TYPE_ALPHABETIC), which would suppress the
    // on-screen keyboard on a keyboard-less TV. What separates a genuinely
    // attached keyboard is externality: isExternal() where available (API
    // 29+), else a real USB/BT vendor/product identity - the built-in
    // claimers are all vendor 0, product 0.
    private static boolean isRealKeyboard(InputDevice d) {
        if (d == null || d.isVirtual()) return false;
        if ((d.getSources() & InputDevice.SOURCE_KEYBOARD) != InputDevice.SOURCE_KEYBOARD) return false;
        if (d.getKeyboardType() != InputDevice.KEYBOARD_TYPE_ALPHABETIC) return false;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) return d.isExternal();
        return d.getVendorId() != 0 || d.getProductId() != 0;
    }

    private static boolean hasHardwareKeyboard() {
        for (int id : InputDevice.getDeviceIds()) {
            if (isRealKeyboard(InputDevice.getDevice(id))) return true;
        }
        return false;
    }

    // Keyboard attach/detach cannot be trusted to fire onConfigurationChanged
    // on TVs (Configuration.keyboard already claims QWERTY, so attaching one
    // changes nothing); listen to input-device hotplug directly. The manifest
    // still declares keyboard|keyboardHidden in configChanges so a config
    // change that does happen restarts nothing.
    private void watchInputDevices() {
        InputManager im = (InputManager) getSystemService(Context.INPUT_SERVICE);
        im.registerInputDeviceListener(new InputManager.InputDeviceListener() {
            @Override
            public void onInputDeviceAdded(int deviceId) {
                nativeHardwareKeyboard(hasHardwareKeyboard());
            }

            @Override
            public void onInputDeviceRemoved(int deviceId) {
                nativeHardwareKeyboard(hasHardwareKeyboard());
            }

            @Override
            public void onInputDeviceChanged(int deviceId) {
                nativeHardwareKeyboard(hasHardwareKeyboard());
            }
        }, null);
    }

    // A display change under an unchanged surface: a refresh-rate switch
    // (the panel's peak-rate setting, a battery saver mode). SDL learns the
    // rate from the surface callbacks only, so without this the runtime
    // kept the old period after a switch; the re-push hands SDL the fresh
    // rate through the same path, and SDL's mode-changed event carries it
    // on to the runtime. Rotation and the like fire it too, where the push
    // repeats what the surface change already sent.
    private final DisplayManager.DisplayListener displayListener = new DisplayManager.DisplayListener() {
        @Override
        public void onDisplayAdded(int displayId) {
        }

        @Override
        public void onDisplayRemoved(int displayId) {
        }

        @Override
        public void onDisplayChanged(int displayId) {
            if (displayId != Display.DEFAULT_DISPLAY || mSurface == null) return;
            mSurface.pushScreenResolution();
        }
    };

    private void watchDisplay() {
        DisplayManager dm = (DisplayManager) getSystemService(Context.DISPLAY_SERVICE);
        dm.registerDisplayListener(displayListener, new Handler(Looper.getMainLooper()));
    }

    @Override
    protected void onDestroy() {
        DisplayManager dm = (DisplayManager) getSystemService(Context.DISPLAY_SERVICE);
        dm.unregisterDisplayListener(displayListener);
        super.onDestroy();
    }

    // The launch facts for native (SDL hands getArguments() to SDL_main as
    // argv): the runtime reports them to the app as env.launch and
    // env.launchLink. Flavors that add arguments of their own extend this
    // list.
    @Override
    protected String[] getArguments() {
        ArrayList<String> args = new ArrayList<>();
        if (restored) {
            args.add("--restored");
        }
        if (launchLink != null) {
            args.add("--link");
            args.add(launchLink);
        }
        return args.toArray(new String[0]);
    }

    // A link arriving while the app runs (the activity is singleInstance, so
    // a second VIEW intent lands here instead of starting another). Handed to
    // native through SDL's own drop path, the same delivery macOS and iOS
    // use for URLs; alloy tells a link from a file drop by its scheme.
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        String link = intent != null ? intent.getDataString() : null;
        if (link != null) {
            SDLActivity.onNativeDropFile(link);
        }
    }

    // Flavor hook, run before SDL comes up: the go client extracts its
    // player assets here; the production runtime does nothing (its payload
    // is read in place from the APK, never extracted).
    protected void prepareAssets() {
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        restored = savedInstanceState != null;
        // A rare lifecycle fact; logged so device traces show which launch
        // the runtime reported (the app's restore decision depends on it).
        Log.v(TAG, "launch " + (restored ? "restored" : "fresh"));
        Intent intent = getIntent();
        if (intent != null && intent.getData() != null) {
            launchLink = intent.getDataString();
            intent.setData(null);
        }
        prepareAssets();
        super.onCreate(savedInstanceState);
        nativeHardwareKeyboard(hasHardwareKeyboard());
        watchInputDevices();
        watchDisplay();

        // Report the IME inset to native whenever insets change (keyboard
        // show/hide). Listens on the content view so it sees the insets before
        // the SDL surface; returns them unconsumed so SDL still gets them.
        //
        // WindowInsets.Type is API 30+, and there is no equivalent IME inset on
        // older releases, so below that we skip the listener entirely and leave
        // the inset at 0 (content simply does not lift for the keyboard).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            View content = findViewById(android.R.id.content);
            content.setOnApplyWindowInsetsListener((v, insets) -> {
                nativeKeyboardInset(insets.getInsets(WindowInsets.Type.ime()).bottom);
                return insets;
            });
        }
    }
}
