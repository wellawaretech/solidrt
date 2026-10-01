# Testing an app

Read this before writing a test for a SolidRT app or package, or before
repairing one that fails. The command reference is `srt test` in cli's
AGENTS.md and src/test/docs.md; the verbs are typed and documented in
`@solidrt/core/src/test.ts` (app tests) and `@solidrt/flux-types/modules/
test.d.ts` (`test`, `expect`, `settle`). This file is the method: what to
test, how to name things, what the time model means for a test, and how to
read a failure.

## The shape

- A test file is `tests/<name>.test.ts` or `.test.tsx` in the package or
  project it tests, never beside the sources. `bunx srt test` finds every
  `tests/` folder under the cwd; `srt check` typechecks them with the app.
- Tests are flat: `test("<a sentence naming the subject>", fn)`, no
  `describe`, no hooks, no mocking framework. Shared setup is a plain
  function a test calls. The file is the group; `--filter <text>` runs a
  subset by name.
- Every test runs in an engine of its own, on the file evaluated afresh:
  module state, timers, listeners, stores and the data folder of one test
  are not there for the next, and a test passes or fails the same alone
  or after the others. Keep the file's top level to registering tests and
  cheap setup, since it runs once per test.
- Two layers, and the file's imports pick one. A file that imports only
  headless modules (`flux:test`, `flux:fs`, `flux:http`, ...) is a flux
  test and runs on the `flux` binary, on real time. A file that imports
  `@solidrt/core/test` (or any app runtime module; every `.tsx`) is an app
  test and runs on the dev client, headless, with no wall clock. Pure
  logic (a parser, a route matcher, a reducer) is a flux test; anything
  that mounts UI, sends input, or needs time to pass is an app test.
- Nothing waits, nothing retries. A flaky test is a bug in the test or in
  the harness, never a rate to manage.

## What to test, in order

Prefer the cheapest reader that proves the behavior, and stop there:

1. Tree and props. `locator.text`, `.box`, `.props`, `.visible`,
   `.exists`. A tapped counter shows "1"; a drawer's box is off screen; a
   disabled button has `opacity: 0.4`. Most tests end here.
2. The outline. `locator.outline()` is the subtree one node per line
   (kind, label, text, box; `{ props: true }` adds props), with no ids:
   pin a whole layout against a string, or print it while writing a test.
3. The GPU inventory. `app.gpu({ label })` says what draws and with what
   (counts, uniforms, formats). "Does the shadow pass run" is a count, not
   a lighting experiment.
4. Pixels. `locator.pixel(x, y)` and `locator.pixels()`, drawn now with no
   time passing. The last resort, for what only the picture shows (a
   shader, a gradient, a blend); they depend on the GPU.

Test behavior through what a user perceives: tap the button the user taps,
assert the text the user reads. Reach into state only to put the app
somewhere its UI reaches slowly (see "Seeding state").

## Writing an app test

```tsx
import { test, expect } from "@solidrt/core/test"
import { Counter } from "../src/counter.tsx"

test("a tap increments", async app => {
  let counter = await app.mount(() => <Counter />)
  await app.tap(counter.find({ text: "Increment" }))
  expect(counter.find({ text: "1" }).visible).toBe(true)
})
```

- Start: `app.mount(ui)` mounts a component (content is put in a window of
  its own) and returns a locator for the window; `app.load(() =>
  import("../src/index.tsx"))` starts the whole entry the way the runtime
  does. Once per test: a test is one app. The `ui` callback is the
  app's component body: anything that registers a cleanup (a pointer
  feed, an input map, a GPU buffer, a scene, a sprite layer) is created
  inside it, as an app creates it inside a component, and handed out
  through a variable the test reads afterwards. Created at the top of
  the test, outside any owner, its cleanup fires unowned and
  `app.settle()` never ends.
- Name nodes the way a user would: `find({ text: "Save" })`. Give a node
  with nothing visible to find it by a `label` prop (`<view
  label="sidebar">`), a stable name that does nothing else. For JSX the
  test writes itself, `let box = app.ref()` and `<view ref={box} />`
  makes `box` both the ref and a locator. A string matches exactly, a
  RegExp a part; `kind` narrows. A locator is resolved each time it is
  used, so it stays valid across frames, and names exactly one node:
  `findAll` for the plural, `.exists` for a yes/no that never throws.
- Act through the real pipeline: `app.tap(locator)`, `app.drag(from, to, {
  durationMs })`, `app.key("Enter")`, `app.type("text")`, `app.input([...])`
  in the control API's `send_input` shape. A tap on a node that something
  covers at its center throws and names the cover (a modal left open is a
  real bug, not a test problem); `app.tap({ x, y })` lands on whatever is
  there. Each verb returns after the frame its last event landed in, so
  the tree is current when it returns.
- Let time pass on purpose: `await app.frame(n)` runs frames,
  `await app.advance(ms)` runs as many as cover the time, `await
  app.settle()` runs the app until it is at rest. After an action that
  loads or animates, `settle()` is the wait, not a guessed `frame(10)`.
- Assert with `expect`: its messages print expected and received, which
  `if (!ok) throw` does not.

## Time, and the traps it removes

An app test has no wall clock. Time is the frames the test asks for, at 60
fps unless `test(name, fn, { fps })` says otherwise; nothing runs between
two frames. Timers fire with the frame their time falls in, so the frame
is the time resolution: a timer due at 500 ms fires at the frame at 500
ms, one due at 505 ms at 516.7 ms. A test that asserts a threshold to the
millisecond sets `{ fps: 1000 }`. `app.time` is the app time so far.

`performance.now()` reads 0, so logic that measures with it fails the same
way every run instead of passing within a tolerance: take time from the
event's `timeStamp` or the frame tick instead. `Date.now()` and `new
Date()` start at 2000-01-01T00:00:00Z (UTC) and move with the frames, so a
countdown or an elapsed-time display is testable. `Math.random()` is
seeded, so random inputs and code that draws random numbers do the same
every run; `--seed <n>` tries another sequence and the report names it.

`settle()` ends when nothing the app started is in flight (a fetch, a
file read, a query), no timer is due, and no frame is demanded. A
demanded frame is run, so a transition plays to its end and the test
reads the end state. What stands is not waited for: a server, an open
socket, a timer due later (`advance` reaches it). An app that never rests
(an `onFrame` loop, a looping animation, a playing video) fails the settle
after 5000 ms of app time, naming what still wanted frames; `settle({
maxMs })` sets another cap. Do not settle an app that animates by itself;
step it with `frame`.

The network is real: a backend is faked with `serve` from `flux:http` plus
`fetch` over loopback, and `settle()` waits for the fetch. Such a backend
answers at once: a `setTimeout` inside its handler is an app timer, which
fires only when the test steps a frame, while `settle()` is waiting for
the fetch with no frame stepped. A delay belongs in the test (`await
app.advance(300)` before the settle) or in the app's own reaction to the
response. The data folder and the fetch cache are empty at the start of
every test, and no gamepad is seated (a `gamepad` event in `app.input`
seats one).

## Seeding state

An app whose UI reaches a state slowly, or not at all headlessly, exposes
the state another way, and the test uses it:

- `registerDebug(name, fn)` from `srt:dev` in the app; `await
  app.debug(name, args)` in the test calls it, runs a frame and returns
  what it returned. The same commands an agent calls over MCP.
- `await app.link("myapp://item/42")` delivers a link the way the OS does
  (throws when the app has no `onLink`); with a router, that renders the
  screen the link names.
- Arguments after `--` on `srt test` reach the file as `flux:process`
  argv.

## Reading a failure

A failed test prints, in this order: the error (expected and received for
an `expect`, the thrown error otherwise) with its source line, then the
details read from the test's engine at the moment of failure, then what
the test logged.

- `In flight: 1 fetch` - work the app had started and not finished. On a
  "Timed out" failure this is what the test waited on; on an assertion
  failure it often means a missing `await app.settle()`.
- `At: 50 ms of app time, frame 3` - when it failed; compare with the
  timers and transitions in play.
- `Frames demanded by: a transition on view labelled "panel"` - the app
  was still moving: a transition, an `onFrame`, a video. An assertion on
  an end state needs a `settle()` first.
- `Outline:` - the whole app as `outline()` prints it, so the layout is
  read without re-running (cut after 200 lines; `locator.outline()` reads
  a subtree in full).
- `Snapshot: dist/test/<file>/failures/<test>.png` - the frame as a user
  would have seen it, written under the project's build output. Open it.
- `No node matches { text: "Goodbye" }; texts there: ...` - a `find` that
  matched nothing lists the texts and labels that were there instead.
- `Timed out after 30000 ms` - the test never finished: a promise nothing
  settles, a synchronous loop, or work in flight (see above). The cap is
  wall time and is not a wait; a test that finishes is never held to it.

An uncaught error (a throw in a timer callback, a rejection nobody
handles) fails the test it happened in. A thrown error inside a component
replaces the app with the error window, which the outline and the
snapshot then show: the test's own error says what was asked, the outline
says what the app did.

## Flux tests

```ts
import { test, expect, settle } from "flux:test"
import { serve } from "flux:http"

test("the server answers with the stored row", async () => {
  let server = serve({ port: 0, fetch: handler })
  let res = await fetch(`http://127.0.0.1:${server.port}/rows/1`)
  expect(res.status).toBe(200)
  server.close()
})
```

A flux test lives on real time: timers, `performance.now()` and
`Date.now()` are the wall's. Logic with a timeout is tested by waiting for
it, or takes its delay as a parameter so the test passes a short one.
`settle()` waits for work the test set off without holding its promise.
Every headless module is available; a listening server ends with the
test's engine. A test that needs time to pass in SolidRT logic is an app
test, whether or not it mounts anything.

## Running

- `bunx srt test` - everything under the cwd; `bunx srt test <dir|file>`
  narrows; `--filter <text>` by name; `--seed <n>` another random
  sequence; `--only flux` or `--only app` one layer (CI runs the flux
  tests on every platform and the app tests where the dev client builds).
- Exit code: nonzero on any failure or on a file that did not complete.
- An app test needs the dev client (`make client` in a SolidRT checkout,
  or the installed platform package); a flux test the `flux` binary.
