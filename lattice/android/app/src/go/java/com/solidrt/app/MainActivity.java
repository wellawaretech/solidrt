package com.solidrt.app;

import android.content.Intent;
import android.content.res.AssetManager;
import android.util.Log;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

// The go dev client's activity: the shared SolidRT body plus the dev loop's
// two extras - the player assets extracted into filesDir, and the
// dev-server address forwarded from the launch intent.
public class MainActivity extends SolidRTActivity {

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

    @Override
    protected void prepareAssets() {
        extractAssets();
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
