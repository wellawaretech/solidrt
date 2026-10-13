// Debug commands under sol test: `app.debug` calls what the app registered
// through `registerDebug` of `sol:dev`, and the documented lifetime (register
// at a screen's mount, unregister in its onCleanup) holds through the
// engine's teardown. The unregister a cleanup holds is a native function
// alive when the engine ends; it must not keep the command's function from
// being finalized, or the teardown aborts the process on the runtime's leak
// check (a reload killed the client that way). The run completing is the
// assertion for that; the test after it reads the same engine state afresh.

import { test, expect } from "../src/index.ts"
import { createSignal, onCleanup, Show } from "@solidrt/core"
import { registerDebug } from "sol:dev"

function Screen() {
  let un = registerDebug("answer", () => 42)
  onCleanup(un)
  return <view flex={1} />
}

test("a command registered at mount answers, and its cleanup-held unregister survives teardown", async app => {
  await app.mount(() => <Screen />)
  expect(await app.debug("answer")).toBe(42)
  await app.frame(3)
})

test("the unregister removes the command when the screen unmounts", async app => {
  let [shown, setShown] = createSignal(true)
  await app.mount(() => (
    <Show when={shown()}>
      <Screen />
    </Show>
  ))
  expect(await app.debug("answer")).toBe(42)
  setShown(false)
  await app.frame()
  await expect(app.debug("answer")).rejects.toThrow("no debug command 'answer'")
})
