# Coming from Electron

SolidRT builds desktop apps from JavaScript and TypeScript like Electron
does, but there is no browser inside it: no HTML, no CSS, no DOM, and no
Chromium to ship. The UI is a tree of native elements drawn on the GPU, and
the same app runs on Android too.

The UI code uses SolidJS for reactivity. If you build your Electron UI with
React, see [React and SolidJS 2](/guides/react-and-solid/).

## Side by side

| Electron | SolidRT |
| --- | --- |
| `BrowserWindow` | `<window>`, one per app, the root of the tree |
| HTML and CSS | The [elements](/docs/reference/elements/) and [layout](/docs/reference/layout/) props: flexbox and a subset of CSS grid |
| Main process, renderer process, IPC | One process: the UI code calls the runtime's modules directly |
| `fs`, `child_process`, `http`, `net` | `flux:fs`, `flux:subprocess`, `flux:http`, `flux:net` (see [Runtime](/runtime/)) |
| `fetch` | `fetch`, the same standard API |
| electron-builder | `sol pack`: one executable (see [The basics](/guides/the-basics/)) |
| DevTools | `sol mcp`: a coding agent reads the live tree, logs and screenshots of the running app (see [Tools](/tools/)) |

## What doesn't carry over

Packages written for the browser DOM won't run: there is no `document`.
Plain TypeScript libraries work as usual.

Styling is props on elements instead of stylesheets. A `view` never paints,
so a background is a `<rect>` inside the view rather than a
`background-color`. The [Docs](/docs/) explain the element vocabulary.
