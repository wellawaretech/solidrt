# Basics

This guide shows how to make an app, run it, change it, and pack it into
one executable file. It takes approximately five minutes.

Before you start, install [Bun](https://bun.com/). The project contains all
other necessary software. SolidRT runs on these systems:

- Linux (x64 and arm64).
- macOS (Apple silicon).
- Windows (x64).

## 1. Make a project

```sh
bun create solidrt my-app
cd my-app
```

A picker asks which extensions the app uses. For this guide, push enter.
You then get [Core](/core/), which is in all SolidRT apps. Later, you can
add extensions as usual dependencies.

The command also installs the dependencies. Thus, you do not do a separate
install step.

## 2. Run the app

```sh
bun run dev
```

This command starts the dev server. It also opens a native window that
connects to the server. Keep the server and the window open. The server
monitors your source files. When a source file changes, the window loads
the app again.

## 3. Change the app

Open `src/index.tsx`. This file contains all of the app:

```tsx
import { render, createLinearGradient, safeArea, Logo } from "@solidrt/core"

function App() {
  let backgroundColor = createLinearGradient(0, 0, 1, 1, [
    { offset: 0, color: "#080b16" },
    { offset: 1, color: "#1d2a52" },
  ])

  return (
    <window title="The Solid Runtime">
      <d-rect color={backgroundColor} />
      <view
        flex={1}
        gap={20}
        alignItems="center"
        justifyContent="center"
        paddingTop={safeArea().top}
        paddingBottom={safeArea().bottom}
      >
        <Logo size={300} animation="loop" />
        <text fontSize={40} color="#ccc">The Solid Runtime</text>
      </view>
    </window>
  )
}

render(() => <App />)
```

The app has these parts:

- A `<window>` root.
- A gradient behind all other content.
- A column in the center. The column does not go behind the notch.

Change the text and save the file. The same window loads the app again and
shows the change.

## 4. Pack the app

First, give the app an id that does not change. The last part of the id is
the name of the output file. On Android, the id is the package name. The
app's storage also uses the id as its key. In `package.json`, replace the
`null` value of `appId`:

```json
"solidrt": {
  "appId": "com.example.myapp",
  ...
}
```

Then run this command:

```sh
bun run pack
```

This command writes `dist/myapp` (`dist/myapp.exe` on Windows). This is
one executable file that contains
the runtime and your app. Run it as you run other programs. To make an
Android APK, use `bun run pack --apk`. Packing is experimental. The
[sol pack](/tools/pack/) reference gives more data about the icon, the
capabilities that the app requests, and the other outputs.

## Next steps

- [Dev loop](/guides/dev-loop/): make the app respond to input, and run
  it on your phone. This guide also shows more about the edit, save and see
  loop.
- Do you know a different framework? These guides compare it to SolidRT:
  [Coming from HTML](/guides/coming-from-html/),
  [Coming from React](/guides/coming-from-react/),
  [Coming from Expo/React Native](/guides/coming-from-react-native/) and
  [Coming from Electron](/guides/coming-from-electron/).
- [Core](/core/): the elements, layout, input, and how reactive props get
  to native nodes.
