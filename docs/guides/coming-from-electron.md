# Coming from Electron

As with Electron, you build desktop apps with JavaScript and TypeScript in
SolidRT. But SolidRT does not contain a browser:

- No HTML, no CSS and no DOM.
- No Chromium to ship.
- One process: no IPC between the UI and the system.

The UI is a tree of native elements that the GPU draws. The basic elements
combine parts of HTML, SVG and canvas.
[Coming from HTML](/guides/coming-from-html/) gives an overview of the
elements.

The same app runs on these platforms:

- Windows (x64).
- macOS (Apple silicon).
- Linux (x64 and arm64).
- Android.

The UI code uses SolidJS for reactivity. If you build your Electron UI with
React, see [Coming from React](/guides/coming-from-react/).

## One process

An Electron app has a main process and a renderer process for each window.
The processes send messages through IPC. You write a preload script, and you
use `contextBridge`, `ipcMain` and `ipcRenderer` to connect the UI to Node.

A SolidRT app is one process. The UI code imports the runtime modules and
calls them directly. You do not write a preload script or a message channel.

To keep long work away from the UI, use `flux:isolate`. An isolate runs a
module on its own thread in the same process. You call its exports as async
functions.

## One executable file

Electron apps use electron-builder to make an installable app. In SolidRT,
`sol pack` makes one executable file that contains the runtime and your app.
Refer to [Basics](/guides/basics/).

`sol pack` makes the executable file for the platform that it runs on. Thus,
to make the Windows file, run `sol pack` on Windows. To make an Android APK,
use `sol pack --apk`. This command works on all platforms.

The executable file is smaller than an Electron app, because it contains no
browser. The size depends on the runtime. With a
[custom runtime](/guides/custom-runtime/), you build in only the features
that your app uses.

## Node modules

The runtime does not have the Node built-in modules. In place of them, use
the runtime modules:

| Node | SolidRT |
| --- | --- |
| `fs` | `flux:fs` |
| `path` | `flux:path` |
| `child_process` | `flux:subprocess` |
| `http` | `flux:http` |
| `net` | `flux:net` |
| `dialog` (Electron) | `flux:dialog` |
| `clipboard` (Electron) | `navigator.clipboard` (text only) |

`fetch` is the same standard API. [Runtime](/runtime/) shows all modules.

Native Node addons do not run. For native code, build a
[custom runtime](/guides/custom-runtime/). For portable compute code, use
`flux:wasm`.

## What does not work the same

Packages for the browser DOM do not run, because there is no `document`.
Packages that use the Node built-in modules (`fs`, `path`, `crypto`,
`Buffer`) also do not run. Packages that use only standard JavaScript and
the web APIs work as usual.

To set the style, you use props on elements, not stylesheets. A `view` does
not paint. Thus, for a background, put a `<d-rect>` in the view. Do not use
`background-color`. The [element vocabulary](/core/vocabulary/) shows all
elements.

There are no DevTools. In place of DevTools, `sol mcp` lets a coding agent
read the live tree, the logs and screenshots of the running app. Refer to
[Tools](/tools/).

## What SolidRT does not have

- More than one window. The `<window>` element is the root of the tree, and
  an app has one.
- Native menus, tray icons and system notifications.
- Auto-update. A new version of the app is a new executable file.
