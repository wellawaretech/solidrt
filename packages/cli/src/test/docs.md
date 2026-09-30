# srt test

{{ usage test }}

Runs a project's tests on the runtime the code ships on, not under Bun, so
a test that passes here passes where the code runs. Each test file is
bundled and run by the binary its imports name: the `flux` binary for a
flux program, the dev client (headless) for a file that imports the app
runtime. Every test runs in an engine of its own.

```sh
srt test                       # every tests/*.test.ts under the current folder
srt test packages/router       # the same, under that folder
srt test tests/route.test.ts   # one file
srt test tests/counter.test.tsx  # an app test, on the dev client
srt test --filter matchPath    # only the tests whose name contains the text
srt test --seed 12345          # the same tests on another Math.random sequence
```

A test file lives in a `tests/` folder of its package or project, never
beside the sources, and is named `<something>.test.ts` (or `.test.tsx`). A
test of plain logic or of a flux program registers its tests with
`flux:test`:

```ts
import { test, expect } from "flux:test"
import { matchPath } from "../src/route.ts"

test("matchPath: a param segment matches any value", () => {
  expect(matchPath("/users/$id", "/users/7")).toEqual({ id: "7" })
})
```

Tests are flat. There is no `describe` and there are no hooks: the file is
the group, the name is a sentence that names its subject, and shared setup
is a plain function a test calls. `--filter` is how a subset is run. The
matchers are listed with the module, in the
[Runtime reference](/runtime/modules/).

A test runs on real time: its timers, `performance.now()` and `Date.now()`
are the wall clock's. Logic with a timeout is tested by waiting for it, or
takes its delay as a parameter, so a test can pass a short one.

Each file runs in a fresh process, one file after another, with the folder
that holds its `tests/` folder as the working directory, so a relative path
in a test means the same wherever the command was started. Inside it the
file is evaluated once to list its tests and once more for every test, in
a fresh engine: a test starts from the file's freshly evaluated state, so
what one test changed, registered or left running is not there for the
next, and a test passes or fails the same whether it runs alone or after
the others. The file's top level runs once per test for that; keep it to
registering tests and cheap setup.

Nothing is retried. A failure prints the expected and the received value
and the source line; what a test printed is shown under it. An uncaught
error (a throw in a timer callback, a rejection nobody handles) fails the
test it happened in. A test that does not finish within 5 seconds fails as
timed out, a synchronous loop included, and one that waits on a promise
nothing will settle fails at once, so a test cannot pass by never
finishing. The command exits nonzero on any failure.

`Math.random()` is seeded in every test's engine. A test that draws random
inputs, and code under test that calls `Math.random()`, therefore does the
same on every run, whatever tests ran before it. `--seed <n>` runs the tests on another sequence, to try more
inputs; a failure found that way comes back with the same number:

```sh
srt test tests/pick.test.ts --seed 12345
```

Everything after `--` reaches the test file as its `flux:process` argv.

A test can import every headless module (`flux:fs`, `flux:http`,
`flux:sqlite`, ...): a server under test is started with `serve` and called
with `fetch` over loopback.

## App tests

A test that imports `@solidrt/core/test` is an app test. It runs on the dev
client without a window, and it has no wall clock: time is the frames the
test asks for.

```ts
import { test, expect } from "@solidrt/core/test"

test("the hint shows after half a second", async app => {
  let shown = false
  setTimeout(() => (shown = true), 500)
  await app.advance(499)
  expect(shown).toBe(false)
  await app.frame()
  expect(shown).toBe(true)
}, { fps: 1000 })
```

`app.frame(n)` runs frames, `app.advance(ms)` runs as many as cover that
much time, and `app.time` is the app time so far. Nothing runs between two
frames: a timer fires with the frame its time falls in, so the frame is
the time resolution of a test (16.7 ms at the default 60 fps; a test that
asserts a threshold to the millisecond sets `{ fps: 1000 }`).
`performance.now()` reads 0, and the calendar starts at
2000-01-01T00:00:00Z, in UTC, and moves with the frames. Every test starts
at time 0 in an engine of its own, like a flux test.

Anything that imports the app runtime (`srt:` modules, or a module of the
rendering layer such as `flux:rendertree`) makes a file an app test; a
`.test.tsx` file always is one. The bundle and a data folder of the file's
own are staged under the project's `dist/test/`.

`srt check` typechecks the test files along with the entries.
