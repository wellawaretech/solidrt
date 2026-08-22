# Start

Build and run your first SolidRT app: scaffold, run, edit, see it update.
Five minutes, one scroll.

You need [Bun](https://bun.com/) installed. Everything else arrives with the
project.

## 1. Scaffold a project

```sh
bun create solidrt@latest my-app
cd my-app
```

A picker asks which packages the app takes. [Core](/core/) is always in:
it is the runtime, the element vocabulary, reactivity, layout, and input that
every SolidRT app has. On top of it you can tick extensions:

- **@solidrt/components** - the [Components extension](/extensions/):
  widgets, theming, navigation. Selecting it scaffolds the starter app built
  with the component set.
- **@solidrt/3d** - a general purpose 3D library: scenes, meshes, materials,
  cameras and lighting on top of Core's GPU primitives.

Pick nothing extra unless you already know you want it; extensions can be
added later as ordinary dependencies.

The picker only appears on an interactive terminal; a script gets core only
and adds extensions afterwards with `bun add`.

Scaffolding installs the dependencies for you, so there is no separate
install step.

## 2. Run it

```sh
bun run dev
```

This starts the dev server and opens a native window connected to it. Leave
both running: the server watches your source, and the window reloads when it
changes.

## 3. Change something

Open `src/index.tsx`. That is the whole app:

```tsx
import { render, createLinearGradient, safeArea } from "@solidrt/core"
import { Icon } from "./icon"

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
        <Icon />
        <text fontSize={40} color="#ccc">The Solid Runtime</text>
      </view>
    </window>
  )
}

render(() => <App />)
```

A `<window>` root, a gradient fill behind everything, and a centered column
that keeps clear of the notch. `src/icon.tsx` holds the animated mark. Around
them the scaffold wrote a `tsconfig.json`, an `AGENTS.md` for coding agents,
the `.mcp.json` that lets one attach to the running app, and `assets/` with
the app icon.

Change the text and save. The running window updates without restarting.

That is the loop: edit, save, see it. Everything else builds on it.

## 4. Make it react

The logo animates, but nothing above responds to you yet. Replace the
contents of `src/index.tsx` with a signal and a pointer handler:

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

Save, click the window, and the number changes. No component re-ran: reading
`count()` inside the JSX subscribed that one piece of text to that one
signal, and the update writes straight to the native node. The rest of Core
is built on that single idea.

## Where next

- **Understand the model:** [Core](/core/) covers the element vocabulary,
  how reactive props reach native nodes, layout, and input.
- **Build faster:** [Extensions](/extensions/) are ready-made component sets
  built on Core, for when you want buttons rather than rectangles.
- **On a device:** the same project runs on a connected Android phone or
  tablet with `bun run android`. See [Tools](/tools/).
