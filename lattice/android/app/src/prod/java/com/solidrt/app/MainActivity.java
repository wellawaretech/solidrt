package com.solidrt.app;

// The production runtime's activity: the shared SolidRT body and nothing
// else. No asset extraction (the packed payload is read in place from the
// APK) and no dev-server intent extra (there is no dev connection in a
// shipped app).
public class MainActivity extends SolidRTActivity {
}
