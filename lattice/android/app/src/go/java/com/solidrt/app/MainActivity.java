package com.solidrt.app;

import android.Manifest;
import android.content.pm.PackageManager;
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
    private static final int CAMERA_PERMISSION_REQUEST = 100;

    private QrScanner qrScanner;

    private static native void nativeOnQrScanned(String content);

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

        // if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
        //     Log.i(TAG, "Camera permission already granted");
        //     startQrScanner();
        // } else {
        //     Log.i(TAG, "Requesting camera permission");
        //     requestPermissions(new String[] { Manifest.permission.CAMERA }, CAMERA_PERMISSION_REQUEST);
        // }
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        // if (requestCode == CAMERA_PERMISSION_REQUEST
        //         && grantResults.length > 0
        //         && grantResults[0] == PackageManager.PERMISSION_GRANTED) {
        //     startQrScanner();
        // } else {
        //     Log.w(TAG, "Camera permission denied");
        // }
    }

    @SuppressWarnings("unused") // Called from native code via JNI
    public void stopQrScanner() {
        runOnUiThread(() -> {
            if (qrScanner != null) {
                Log.i(TAG, "Stopping QR scanner (dev server found)");
                qrScanner.stop();
                qrScanner = null;
            }
        });
    }

    private void startQrScanner() {
        qrScanner = new QrScanner(this, content -> {
            Log.i(TAG, "QR scanned: " + content);
            nativeOnQrScanned(content);
            runOnUiThread(() -> {
                if (qrScanner != null) {
                    Log.i(TAG, "Stopping QR scanner after successful scan");
                    qrScanner.stop();
                    qrScanner = null;
                }
            });
        });
        qrScanner.start();
    }

    @Override
    protected void onDestroy() {
        if (qrScanner != null) {
            qrScanner.stop();
            qrScanner = null;
        }
        super.onDestroy();
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