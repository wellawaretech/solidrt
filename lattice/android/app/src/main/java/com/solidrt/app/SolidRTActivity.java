package com.solidrt.app;

import android.content.Context;
import android.hardware.input.InputManager;
import android.os.Build;
import android.os.Bundle;
import android.view.InputDevice;
import android.view.View;
import android.view.WindowInsets;

import org.libsdl.app.SDLActivity;

// The SolidRT activity body shared by both flavors: the go dev client and the
// production runtime (each flavor's MainActivity subclasses this; the
// manifest component name com.solidrt.app.MainActivity is the launch
// contract). Owns the native library set and the keyboard facts the runtime
// cannot observe from SDL; flavor-specific behavior (the go client's asset
// extraction and dev-server intent extra) lives in the subclasses.
public class SolidRTActivity extends SDLActivity {
    protected static final String TAG = "SolidRT";

    @Override
    protected String[] getLibraries() {
        return new String[] {
            "SDL3",
            "impeller",
            "main"
        };
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

    // Flavor hook, run before SDL comes up: the go client extracts its
    // player assets here; the production runtime does nothing (its payload
    // is read in place from the APK, never extracted).
    protected void prepareAssets() {
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        prepareAssets();
        super.onCreate(savedInstanceState);
        nativeHardwareKeyboard(hasHardwareKeyboard());
        watchInputDevices();

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
