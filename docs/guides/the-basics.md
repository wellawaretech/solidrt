# The basics

Scaffold an app, run it, change it, and pack it into a standalone
executable. Five minutes, one scroll.

You need [Bun](https://bun.com/) installed. Everything else comes with the
project.

## 1. Scaffold a project

```sh
bun create solidrt@latest my-app
cd my-app
```

A picker asks which extensions the app takes. Ignore it for now: press enter
and you get [Core](/core/), which every SolidRT app has. You can add
extensions later as ordinary dependencies.

Scaffolding installs the dependencies, so there is no separate install step.

## 2. Run it

```sh
bun run dev
```

This starts the dev server and opens a native window connected to it. Keep
both running: the server watches your source, and the window reloads when
the source changes.

## 3. Change something

Open `src/index.tsx`. That is the whole app:

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

A `<window>` root, a gradient behind everything, and a centered column that
stays clear of the notch.

Change the text and save. The running window updates without restarting.

## 4. Pack it

Give the app a stable id first. It names the packed app, and the app's
storage is keyed by it. In `package.json`:

```json
"solidrt": {
  "appId": "com.example.myapp",
  ...
}
```

Then:

```sh
bun run pack
```

This writes `dist/myapp`, one executable with the runtime and your app
inside. Run it like any other program. `bun run pack --apk` builds an
Android APK instead. Packing is still experimental. The
[sol pack](/tools/pack/) reference covers the icon, the capabilities the
app asks for, and the other outputs.

## Where next

- [The dev loop](/guides/dev-loop/): make the app react to input, run it on
  your phone, and get the most out of the edit-save-see loop.
- Know another framework? [React and SolidJS 2](/guides/react-and-solid/),
  [Coming from React Native](/guides/coming-from-react-native/) and
  [Coming from Electron](/guides/coming-from-electron/) map what you know.
- [Core](/core/): the element vocabulary, layout, input, and how reactive
  props reach native nodes.
