# {{name}}

A SolidRT app: SolidJS 2.0 rendered natively through a Rust runtime.
Bootstrapped with `bun create solidrt`.

## Step 1: Start the app

```sh
bun run dev
```

This starts the dev server and opens your app in a window.

## Step 2: Modify your app

Open `src/index.tsx`, change something and save. The window updates on its
own.

## Step 3: Pack your app

```sh
bun run pack
```

This builds a standalone executable into `dist/` that runs without the dev
server. `bun run pack --apk` builds an installable Android APK instead.
Before you share it, set a stable `appId` in the `"solidrt"` section of
`package.json` (for example `com.example.myapp`).

## Step 4: Run it on your Android device

Uses `adb`, from Android Platform Tools; if you don't have it,
`bun run android` offers to download it. No extra tooling required: no
Android SDK, Android Studio or Gradle.

Connect an Android device over USB with debugging enabled, keep `bun run dev`
running, and in a second terminal:

```sh
bun run android
```

The first time, this installs the SolidRT client on the device. Every save
now reaches the window and the device together.

## Make it yours

- `assets/icon.svg` and `assets/icon.png` are the app icon. Everything in
  `assets/` ships with the app.
- The `"solidrt"` section of `package.json` holds the app's name, id and
  capabilities.

## Available scripts

| Command           | What it does                                                |
| ----------------- | ----------------------------------------------------------- |
| `bun run dev`     | Dev server plus a local client window                       |
| `bun run server`  | Dev server only, for clients on other devices               |
| `bun run client`  | Attach a client window to the running dev server            |
| `bun run android` | Launch the client on a connected Android device             |
| `bun run check`   | Build and typecheck without producing output (good for CI)  |
| `bun run test`    | Run the tests in `tests/` (not `bun test`)                  |
| `bun run pack`    | Package a standalone build into dist/ (`--apk` for Android) |

## Learn more

https://github.com/wellawaretech/solidrt
