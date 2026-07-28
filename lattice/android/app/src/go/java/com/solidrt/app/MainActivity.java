package com.solidrt.app;

import android.content.Intent;
import android.content.res.AssetManager;
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

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // TEMPORARY (swap-latency diagnosis): raise native log level so the
        // raster phase trace (alloy FrameTiming, log::debug) reaches logcat.
        // Remove when okf/backlog/android-surface-swap-latency.md closes.
        try {
            android.system.Os.setenv("SRT_LOG", "debug", true);
        } catch (Exception e) {
            Log.w(TAG, "setenv SRT_LOG failed", e);
        }
        // TEMPORARY (swap-latency diagnosis): forward the srt_swap_interval
        // intent extra into the env so alloy's present path can be A/B
        // switched per launch (see sdl_utils::window_swap_interval).
        try {
            Intent intent = getIntent();
            String si = intent != null ? intent.getStringExtra("srt_swap_interval") : null;
            if (si != null && !si.isEmpty()) {
                android.system.Os.setenv("SRT_SWAP_INTERVAL", si, true);
            }
            String gf = intent != null ? intent.getStringExtra("srt_gl_finish") : null;
            if (gf != null && !gf.isEmpty()) {
                android.system.Os.setenv("SRT_GL_FINISH", gf, true);
            }
        } catch (Exception e) {
            Log.w(TAG, "setenv SRT_SWAP_INTERVAL failed", e);
        }
        extractAssets();
        super.onCreate(savedInstanceState);

        // Report the IME inset to native whenever insets change (keyboard
        // show/hide). Listens on the content view so it sees the insets before
        // the SDL surface; returns them unconsumed so SDL still gets them.
        View content = findViewById(android.R.id.content);
        content.setOnApplyWindowInsetsListener((v, insets) -> {
            nativeKeyboardInset(insets.getInsets(WindowInsets.Type.ime()).bottom);
            return insets;
        });
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