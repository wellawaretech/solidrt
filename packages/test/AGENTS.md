# @solidrt/test - agent notes

Tests for a SolidRT app: `test` registers one and hands it the app under
test, `expect` asserts. A devDependency of every app and a peer of
@solidrt/core. This file is the API; the method (what to test at which
layer, when to `settle`, how to read a failure) is @solidrt/cli
agents/testing.md.

- A test file is `tests/<name>.test.tsx`, run with `bun run srt test`. It
  imports `test` and `expect` from `@solidrt/test`; every test gets
  the app under test and an engine of its own:

  ```tsx
  import { test, expect } from "@solidrt/test"

  test("a tap increments", async app => {
    let counter = await app.mount(() => <Counter />)
    await app.tap(counter.find({ text: "Increment" }))
    expect(counter.find({ text: "1" }).visible).toBe(true)
  })
  ```

- `app.mount(ui)` takes what `render` takes (content is put in a window of
  its own) and returns a locator for the window; `app.load(() =>
  import("../src/index.tsx"))` starts a whole entry instead. Once per test.
- Name a node by what the user sees, `find({ text: "Save" })`; by its
  `label` prop where it shows no text, `find({ label: "sidebar" })`
  (`label` is on every element, a stable name and nothing else); or hold a
  ref, `let box = app.ref()` with `<view ref={box} />`. A string matches
  exactly, a RegExp a part; `kind` narrows. A locator is resolved each time
  it is used and names exactly one node: reading through one that matches
  none or several throws (the message lists the texts and labels that are
  there), `findAll` returns them all, `.exists` never throws.
- Read `.text`, `.box` (painted, window coordinates), `.props` (off-default,
  JSX names), `.visible` (painted: not clipped away by a scrolling or
  hidden-overflow ancestor or the window, not faded to 0, mounted) and
  `.record` (the `/tree` record).
- Input is the real pipeline: `app.tap(locator)`, `app.drag(from, to, {
  durationMs })`, `app.key("Enter")`, `app.type("text")` (to the focused
  node), `app.input([...])` for raw events in the `send_input` shape. A tap
  on a node that something covers at its center throws and names the
  cover; a point (`{ x, y }`) lands on whatever is there. Each verb returns
  after the frame its last event landed in, so the tree is current.
- There is no wall clock: time passes only by `app.frame(n)` and
  `app.advance(ms)`, timers fire with the frame their time falls in,
  `performance.now()` is 0 and the date starts at 2000-01-01 UTC and moves
  with the frames. Nothing waits and nothing is retried. `{ fps: 1000 }`
  as the third argument of `test` makes a frame one millisecond.
- `await app.settle()` runs the app until it is at rest: what it started
  (a fetch, a file read, a query) has landed, no timer is due and no frame
  is demanded. Use it after an action that loads or animates, in place of
  a guessed `frame(n)`: a transition is played to its end. A timer due
  later is not waited for (`advance` reaches it). An app that never rests
  (an `onFrame` loop, a looping animation) fails it after 5000 ms of app
  time (`{ maxMs }` for another cap), naming what still wanted frames.
- `await app.link(link)` delivers a link as the OS does (throws when the
  app has no `onLink`); `await app.debug(name, args)` calls a command the
  app registered with `registerDebug` and returns its value: the way to
  put a loaded app into a state its UI reaches slowly. A `gamepad` event
  in `app.input` seats and drives a synthetic pad, as `send_input` does.
- Past a node's record: `locator.outline()` is the subtree as an outline
  (kind, label, text, box per line, no ids) to pin a layout or print it;
  `locator.pixel(x, y)` is `[r, g, b, a]` at a point of the node and
  `locator.pixels()` the whole image, drawn now with no time passing (the
  last resort, for what only the picture shows); `app.gpu({ label })` is
  the GPU inventory `/gpu` reports, where "does it draw" is a count.
- The data folder and the fetch cache are empty at the start of every
  test, and no gamepad is seated.
- A failure prints the app time and frame, what still demanded frames,
  the whole outline and a snapshot's path under `dist/test/` beside the
  error: read them before changing the test. The method (what to test at
  which layer, when to `settle`) is @solidrt/cli agents/testing.md.
