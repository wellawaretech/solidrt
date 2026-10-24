# Dev loop

Edit, save, see the result. This guide starts at the end of
[Basics](/guides/basics/). You have a project, and `bun run dev`
runs.

## Make the app respond to input

The loop also works for behavior, not only for text. Replace the contents of
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

Save the file and click the window. The number increases. No component runs
again. The JSX reads `count()`, and this read connects that one text to that
one signal. Each update writes directly to the native node.
[Core](/core/) gives a full description of this model.

## What causes a reload

The server monitors each file that it used to build the running bundle:

- Your modules.
- The dependencies in the bundle.
- `package.json` and `tsconfig.json`.
- The `assets/` folder.

When you save one of these files, all connected windows load the app again.
A file that the app does not import does not cause a reload.

A reload starts the app again. Thus, the count in the example above goes
back to 0.

The `console.log` output from the local window shows in the terminal where
you started `bun run dev`. The stack traces refer to your `.tsx` source.

## Run the app on your phone

The `dev` script starts the server with `--lan`. Thus, devices on your
network can connect to the server. Connect an Android phone or tablet that
has USB debugging on. Keep `bun run dev` open. In a second terminal, run
this command:

```sh
bun run android
```

The first time, the command asks which Android targets the project builds
for:

- arm64-v8a: current phones and tablets.
- armeabi-v7a: older phones and many Android TVs.
- x86_64: emulators and Chromebooks.

Then the command installs the SolidRT client on the device with adb. It
starts the client, and the client connects to your server. adb is part of
Android Platform Tools. If Platform Tools is not installed, the command
can download it into `~/.solidrt`.

After this, when you save a file, the phone and the desktop window load the
app again at the same time. One server controls all clients that connect to
it.

After the first `bun run android`, the SolidRT player stays on the device.
Then the device can connect without a cable. When the server starts, it
shows its address and a QR code. On the device, the player can scan the
code. You can also type the address into the player.

To connect a second computer, open a copy of the project on that computer.
Then run `bun run client --server <address>`.

## When the window does not respond

If the window does not respond, or if you close it, the server continues to
run. This command opens a new window and connects it to the server:

```sh
bun run client
```

To start all again, stop `bun run dev` with Ctrl-C. Then start it again.

## More terminal commands

The terminal that runs `bun run dev` accepts commands at the `sol>` prompt:

- `reload` sends the app again.
- `list` shows the connected clients.
- `whoami` shows the address and its QR code again.
- `watch off` stops the reload when you save.
- `quit` stops the server.

Push Tab to complete a command.

Some flags are for less usual setups:

- `bun run dev --tunnel` connects devices through a peer-to-peer tunnel,
  not through the local network. Use it when the phone cannot connect to
  your computer. With `--tunnel`, the QR code contains the tunnel ticket,
  not the address.
- `bun run dev --proxy-http` sends the app's `fetch` calls through your
  computer and keeps the responses in a cache. Use it for devices that need
  data that only your computer can get.

Before you commit, run `bun run check`. It builds and typechecks all
entries. It does not write output.

The reference pages describe all flags: [sol run](/tools/server/),
[sol client](/tools/client/), [sol android](/tools/android/) and
[sol check](/tools/check/).
