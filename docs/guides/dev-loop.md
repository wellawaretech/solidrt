# The dev loop

Edit, save, see it. This guide picks up where [The basics](/guides/the-basics/)
left off: a project scaffolded and `bun run dev` running.

## Make it react

The loop works for behavior, not just text. Replace the contents of
`src/index.tsx` with a signal and a pointer handler:

```tsx
import { render, createSignal, safeArea } from "@solidrt/core"

function App() {
  let [count, setCount] = createSignal(0)
  return (
    <window>
      <view
        flex={1}
        paddingTop={safeArea().top}
        alignItems="center"
        justifyContent="center"
        onPointerDown={() => setCount(count() + 1)}
      >
        <text fontSize={24}>Tapped {count()} times</text>
      </view>
    </window>
  )
}

render(() => <App />)
```

Save, click the window, and the number goes up. No component ran again:
reading `count()` inside the JSX subscribed that one piece of text to that
one signal, and each update writes straight to the native node.
[Core](/core/) explains this model in full.

## What triggers a reload

The server watches every file the running bundle was built from: your
modules, the dependencies it bundles, `package.json`, `tsconfig.json`, and
the `assets/` folder. Save any of them and every connected window reloads.
A file the app doesn't import never triggers a reload.

A reload restarts the app, so the count above goes back to 0.

`console.log` from the local window prints in the terminal where you ran
`bun run dev`, with stack traces pointing at your `.tsx` source.

## On your phone

The `dev` script runs the server with `--lan`, so devices on your network
can reach it. Connect an Android phone or tablet with USB debugging on, and:

```sh
bun run android
```

The first time, this asks which Android targets the project builds for:
arm64-v8a for current phones and tablets, armeabi-v7a for older phones and
many Android TVs, x86_64 for emulators and Chromebooks. It then installs the
SolidRT client on the device over adb and launches it against your server.
adb comes with Android Platform Tools; if none is installed, the command
offers to download them into `~/.solidrt`.

From then on, a save reloads the phone and the desktop window together: one
server drives every client connected to it. The device can also join without
a cable: at startup the server prints its address and a QR code, and the
SolidRT player on the device can scan the code or take the address typed in.
A second computer joins with `bun run client --server <address>`.

## When something wedges

The server outlives its window. If the window hangs or you close it, the
server keeps running, and

```sh
bun run client
```

attaches a fresh window to it. To restart everything, stop `bun run dev`
with Ctrl-C and start it again.

## More from the terminal

The terminal running `bun run dev` takes commands at the `sol>` prompt:
`reload` pushes the app again, `list` shows the connected clients, `whoami`
reprints the address and its QR code, `watch off` stops reloading on save,
and `quit` stops the server. Tab completes commands.

A few flags for less common setups:

- `bun run dev --tunnel` connects devices through a peer-to-peer tunnel
  instead of the local network, for when the phone can't reach your machine.
- `bun run dev --proxy-http` routes the app's `fetch` calls through your
  machine and caches the responses, for devices that need data only your
  machine can reach.

Before you commit, `bun run check` builds and typechecks every entry
without writing any output.

The reference pages cover every flag: [sol run](/tools/server/),
[sol client](/tools/client/), [sol android](/tools/android/) and
[sol check](/tools/check/).
