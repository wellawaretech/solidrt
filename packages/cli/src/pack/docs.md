# srt pack

{{ usage pack }}

Bundles, compiles to bytecode and appends the result to the runner as one
standalone executable; `--folder` writes the flat runner + manifest +
bundle + assets folder to `dist/pack/` instead. `--flux` packs a script for
the bare [Flux runtime](/runtime/). Experimental.

One output rule: every deliverable defaults into the gitignored `dist/`
build root - the executable, `.srtapp` and `.apk` as `dist/<name>` files
named by the appId's last segment, flow folders (`pack/`, `render/`,
`bundle/`) below it - never next to the sources. `--output` overrides.

`--app` writes the app alone, without a runner: one `<name>.srtapp` holding
the manifest, the bytecode and the assets, which any `solidrt` runner of the
same version runs as `solidrt <file>.srtapp`. The runner is used in place,
so a signed runner stays signed; the file is platform-independent. This is
how the CLI ships the [console](../console/docs.md).

`--apk` writes an installable Android APK, with no Android SDK on the
machine: a copy of the runner APK is patched in place - application id
(from `appId`) and label rewritten, the `.srtapp` payload added - then
re-aligned and re-signed with a fixed development key, so the result
sideloads out of the box (publishing will need a real key). The base is
the production runner APK (in a checkout, `make android-runtime` stages
it), which boots the payload directly - no player, no dev server. With
no runner staged, the solidrt-go dev client stands in: that APK installs
and launches, but boots the player instead of the payload.

Set a stable `appId` in the `solidrt` key of package.json before
distributing (it keys the app's storage folder); `pack` warns while it is
defaulted from the package name.

What the app is and intends sits at the top level of that key, in
platform-neutral terms; the `android` group holds what only Android needs:

```json
"solidrt": {
  "appId": "com.example.app",
  "icon": "./assets/icon.svg",
  "iconBackground": "#1a1a1a",
  "capabilities": {
    "camera": true,
    "microphone": true,
    "vibration": true,
    "internet": true
  },
  "backup": false,
  "android": {
    "versionCode": 1,
    "permissions": ["com.android.vending.BILLING"]
  }
}
```

`iconBackground` is the ground behind the icon's transparent foreground
(the adaptive icon's background layer behind `assets/icon.png`).
`capabilities` is what the app may ask the user for, in platform-neutral
names. All are on unless switched off, so a packed app can do what it did
in the player; the scaffold lists them all, and `"camera": false` (or
deleting the line) drops one. The runner declares no permission of its own:
each capability kept on becomes the APK's permission, and only a declared
permission can be requested at runtime. `android.permissions` adds fully
qualified `<uses-permission>` names beyond those (billing, push, vendor).
`android.versionCode` orders updates and must only ever grow; `versionName`
is the package `version`.

A packed APK keeps its data on the device: it is not debuggable (no `adb
shell run-as` into its files), and neither cloud backup nor device-to-device
transfer copies anything out of it. `backup: true` lets the OS back up the
app's `data/` folder (sqlite, `file()` writes); the fetch cache, logs and the
p2p identity stay out either way.

The appId is also the scheme the packed app answers to: a link like
`com.example.app://settings` opens it (`env.launchLink` on a cold start,
`onLink` while it runs). The APK declares the scheme in its manifest; a
desktop executable registers itself when the app calls
`registerProtocolHandler()`, and a second instance the OS starts with a link
hands the link to the running one and exits.
