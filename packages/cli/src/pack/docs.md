# sol pack

{{ usage pack }}

Bundles, compiles to bytecode and appends the result to the runner as one
standalone executable; `--folder` writes the flat runner + manifest +
bundle + assets folder to `dist/pack/` instead. `--flux` packs a script for
the bare [Flux runtime](/runtime/). Experimental.

One output rule: every deliverable defaults into the gitignored `dist/`
build root - the executable, `.solapp` and `.apk` as `dist/<name>` files
named by the appId's last segment (an APK also by its ABI), flow folders (`pack/`, `render/`,
`bundle/`) below it - never next to the sources. `--output` overrides.

`--app` writes the app alone, without a runner: one `<name>.solapp` holding
the manifest, the bytecode and the assets, which any `solidrt` runner of the
same version runs as `solidrt <file>.solapp`. The runner is used in place,
so a signed runner stays signed; the file is platform-independent. This is
how the CLI ships the [console](../console/docs.md).

`--apk` writes installable Android APKs, with no Android SDK on the
machine: one per Android target the project has (its
`@solidrt/android-<abi>` dev dependencies; the first time, a picker asks
which to add), as `dist/<name>-<abi>.apk`. For each, a copy of the target's
runner APK is patched in place - application id (from `appId`) and label
rewritten, the `.solapp` payload added - then re-aligned and re-signed with
a fixed development key, so the result sideloads out of the box
(publishing will need a real key). The runner boots the payload directly -
no player, no dev server. Every `@solidrt/android-<abi>` package carries
it; in a checkout, `make android-runtime` stages it, and a target with no
runner staged is skipped with a note. `sol android --apk` packs and
installs in one step.

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
  },
  "runtime": "runtime/dist"
}
```

`runtime` names the project's own runtime binaries, when the project builds
a custom runtime (a cargo project over lattice with the project's native
modules; see the runtime docs, "Native code"): a directory in the same
layout as a checkout's `dist/`. `pack`, `run`, `android` and `test` take the
app runtime from there first, `solidrt`, `solidrt-go` and the Android APKs
alike, and fall back to the stock binary for a target the project did not
build. The APKs are derived from the stock ones with the project's `.so`
files swapped in, so the project never carries the Android shell.

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
