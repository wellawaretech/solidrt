package com.solidrt.app;

import android.util.Log;

import androidx.annotation.NonNull;
import androidx.annotation.OptIn;
import androidx.camera.core.CameraSelector;
import androidx.camera.core.ExperimentalGetImage;
import androidx.camera.core.ImageAnalysis;
import androidx.camera.core.ImageProxy;
import androidx.camera.lifecycle.ProcessCameraProvider;
import androidx.core.content.ContextCompat;
import androidx.lifecycle.Lifecycle;
import androidx.lifecycle.LifecycleOwner;
import androidx.lifecycle.LifecycleRegistry;

import com.google.common.util.concurrent.ListenableFuture;
import com.google.mlkit.vision.barcode.BarcodeScanner;
import com.google.mlkit.vision.barcode.BarcodeScannerOptions;
import com.google.mlkit.vision.barcode.BarcodeScanning;
import com.google.mlkit.vision.barcode.common.Barcode;
import com.google.mlkit.vision.common.InputImage;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class QrScanner implements LifecycleOwner {
    private static final String TAG = "SolidRT";
    private static final long THROTTLE_MS = 1000;

    public interface OnQrScannedListener {
        void onQrScanned(String content);
    }

    private final android.app.Activity activity;
    private final OnQrScannedListener listener;
    private final ExecutorService executor = Executors.newSingleThreadExecutor();
    private final BarcodeScanner scanner;
    private final LifecycleRegistry lifecycleRegistry = new LifecycleRegistry(this);
    private ProcessCameraProvider cameraProvider;
    private long lastScanTime = 0;
    private int frameCount = 0;

    public QrScanner(android.app.Activity activity, OnQrScannedListener listener) {
        this.activity = activity;
        this.listener = listener;

        BarcodeScannerOptions options = new BarcodeScannerOptions.Builder()
            .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
            .build();
        this.scanner = BarcodeScanning.getClient(options);
    }

    @NonNull
    @Override
    public Lifecycle getLifecycle() {
        return lifecycleRegistry;
    }

    public void start() {
        Log.i(TAG, "QR scanner: starting...");
        lifecycleRegistry.setCurrentState(Lifecycle.State.STARTED);

        ListenableFuture<ProcessCameraProvider> future = ProcessCameraProvider.getInstance(activity);
        future.addListener(() -> {
            try {
                Log.i(TAG, "QR scanner: camera provider obtained");
                cameraProvider = future.get();
                bindAnalysis();
            } catch (Exception e) {
                Log.e(TAG, "QR scanner: failed to get camera provider", e);
            }
        }, ContextCompat.getMainExecutor(activity));
    }

    public void stop() {
        lifecycleRegistry.setCurrentState(Lifecycle.State.DESTROYED);
        if (cameraProvider != null) {
            cameraProvider.unbindAll();
            cameraProvider = null;
        }
        executor.shutdown();
        scanner.close();
    }

    private void bindAnalysis() {
        ImageAnalysis imageAnalysis = new ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build();

        imageAnalysis.setAnalyzer(executor, this::analyzeImage);

        CameraSelector cameraSelector;
        String cameraName;
        try {
            if (cameraProvider.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA)) {
                cameraSelector = CameraSelector.DEFAULT_BACK_CAMERA;
                cameraName = "back";
            } else {
                cameraSelector = CameraSelector.DEFAULT_FRONT_CAMERA;
                cameraName = "front";
            }
        } catch (Exception e) {
            Log.e(TAG, "Failed to query cameras, defaulting to back", e);
            cameraSelector = CameraSelector.DEFAULT_BACK_CAMERA;
            cameraName = "back";
        }

        cameraProvider.unbindAll();
        cameraProvider.bindToLifecycle(this, cameraSelector, imageAnalysis);
        Log.i(TAG, "QR scanner active, using " + cameraName + " camera, scanning for QR codes...");
    }

    @OptIn(markerClass = ExperimentalGetImage.class)
    private void analyzeImage(@NonNull ImageProxy imageProxy) {
        android.media.Image mediaImage = imageProxy.getImage();
        if (mediaImage == null) {
            imageProxy.close();
            return;
        }

        long now = System.currentTimeMillis();
        if (now - lastScanTime < THROTTLE_MS) {
            imageProxy.close();
            return;
        }

        frameCount++;
        if (frameCount % 30 == 1) {
            Log.d(TAG, "QR scanner: analyzing frame #" + frameCount);
        }

        InputImage image = InputImage.fromMediaImage(mediaImage, imageProxy.getImageInfo().getRotationDegrees());
        scanner.process(image)
            .addOnSuccessListener(barcodes -> {
                for (Barcode barcode : barcodes) {
                    String value = barcode.getRawValue();
                    if (value != null && !value.isEmpty()) {
                        lastScanTime = System.currentTimeMillis();
                        listener.onQrScanned(value);
                        break;
                    }
                }
            })
            .addOnFailureListener(e -> Log.e(TAG, "QR scan failed", e))
            .addOnCompleteListener(task -> imageProxy.close());
    }
}
