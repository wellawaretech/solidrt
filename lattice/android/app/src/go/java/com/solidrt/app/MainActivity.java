package com.solidrt.app;

import android.content.res.AssetManager;
import android.os.Bundle;
import android.util.Log;

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

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        extractAssets();
        super.onCreate(savedInstanceState);
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