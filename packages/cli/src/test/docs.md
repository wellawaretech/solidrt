# srt test

{{ usage test }}

Runs a project's tests on the runtime the code ships on: each test file is
bundled and run by the `flux` binary, not by Bun, so a test that passes
here passes where the code runs.

```sh
srt test                       # every tests/*.test.ts under the current folder
srt test packages/router       # the same, under that folder
srt test tests/route.test.ts   # one file
srt test --filter matchPath    # only the tests whose name contains the text
srt test --seed 12345          # the same tests on another Math.random sequence
```

A test file lives in a `tests/` folder of its package or project, never
beside the sources, and is named `<something>.test.ts`. It registers its
tests with `flux:test`:

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
in a test means the same wherever the command was started. Nothing is
retried. A failure prints the expected and the received value and the
source line; what a test printed is shown under it. A test that does not
finish within 5 seconds fails as timed out, and a file whose process ends
before every test has reported fails as a whole, so a test cannot pass by
never finishing. The command exits nonzero on any failure.

`Math.random()` is seeded in a test file, and every test draws its sequence
from the start. A test that draws random inputs, and code under test that
calls `Math.random()`, therefore does the same on every run, whatever tests
ran before it. `--seed <n>` runs the tests on another sequence, to try more
inputs; a failure found that way comes back with the same number:

```sh
srt test tests/pick.test.ts --seed 12345
```

Everything after `--` reaches the test file as its `flux:process` argv.

A test can import every headless module (`flux:fs`, `flux:http`,
`flux:sqlite`, ...): a server under test is started with `serve` and called
with `fetch` over loopback. Tests of an app's UI (`.test.tsx`) are not
supported yet.

`srt check` typechecks the test files along with the entries.
