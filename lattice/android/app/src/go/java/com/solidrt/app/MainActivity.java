package com.solidrt.app;

import android.content.Intent;
import android.content.res.AssetManager;
import android.content.res.Configuration;
import android.os.Build;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.view.WindowInsets;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

import org.libsdl.app.SDLActivity;

public class MainActivity extends SDLActivity {
    private static final String TAG = "SolidRT";

    @Override
    protected String[] getLibraries() {
        return new String[] {
            "SDL3",
            "impeller",
            "main"
        };
    }

    // The dev CLI (`srt client --android`) passes the dev-server address to dial
    // as an intent extra. Forward it to native as argv (SDL hands getArguments()
    // to SDL_main), where the go client reads --dev-server and auto-connects.
    // Avoids adb reverse, which does not work over wireless adb.
    @Override
    protected String[] getArguments() {
        Intent intent = getIntent();
        String addr = intent != null ? intent.getStringExtra("srt_dev_server") : null;
        if (addr != null && !addr.isEmpty()) {
            return new String[] { "--dev-server", addr };
        }
        return new String[0];
    }

    // Forwards the soft keyboard (IME) inset height in pixels to native. The
    // window is fullscreen/edge-to-edge, so the OS will not resize for the
    // keyboard; the app lifts its own content using this value instead.
    private static native void nativeKeyboardInset(int px);

    // Forwards hardware-keyboard presence to native. SDL's Android backend
    // does not track keyboards, so native cannot see this itself; the runtime
    // uses it to keep the on-screen keyboard down while one is attached.
    private static native void nativeHardwareKeyboard(boolean present);

    private static boolean hasHardwareKeyboard(Configuration config) {
        return config.keyboard != Configuration.KEYBOARD_NOKEYS
            && config.hardKeyboardHidden != Configuration.HARDKEYBOARDHIDDEN_YES;
    }

    // The manifest declares keyboard|keyboardHidden in configChanges, so
    // attach/detach arrives here instead of restarting the activity.
    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        nativeHardwareKeyboard(hasHardwareKeyboard(newConfig));
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        extractAssets();
        super.onCreate(savedInstanceState);
        nativeHardwareKeyboard(hasHardwareKeyboard(getResources().getConfiguration()));

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

    private void extractAssets() {
        Log.v(TAG, "Extracting assets");
        copyDir(getAssets(), "", getFilesDir());
    }

    private void copyDir(AssetManager am, String path, File dest) {
        try {
            String[] list = am.list(path);
            if (list == null) return;

            if (list.length == 0) {
                File outFile = new File(dest, path);
                outFile.getParentFile().mkdirs();
                InputStream in = am.open(path);
                OutputStream out = new FileOutputStream(outFile);
                byte[] buf = new byte[8192];
                int len;
                while ((len = in.read(buf)) > 0) {
                    out.write(buf, 0, len);
                }
                out.close();
                in.close();
                Log.v(TAG, "Extracted: " + path);
            } else {
                for (String child : list) {
                    String childPath = path.isEmpty() ? child : path + "/" + child;
                    copyDir(am, childPath, dest);
                }
            }
        } catch (Exception e) {
            Log.e(TAG, "Failed to extract: " + path, e);
        }
    }
}