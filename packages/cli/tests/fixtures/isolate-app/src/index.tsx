// A fixture app for the sol test suite (packages/cli/src/test/main.ts): an
// app that moves work into an isolate, as performance.md advises, and
// shows the result. Its test loads it the way the runtime starts it and
// expects the isolate to run under the test like it does under `sol run`.

import { createSignal, render } from "@solidrt/core"
import { isolate } from "flux:isolate"
import type * as Mesher from "./mesher.ts"

let mesher = isolate<typeof Mesher>("mesher")
let [status, setStatus] = createSignal("working")
mesher.sum([1, 2, 3]).then(
  (total) => setStatus(`sum: ${total}`),
  (e: Error) => setStatus(`failed: ${e.message}`),
)

render(() => (
  <window>
    <text>{status()}</text>
  </window>
))
