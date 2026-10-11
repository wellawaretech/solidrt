import { test, expect } from "@solidrt/test"
import { hello } from "runtime:hello"

// Runs on the app's own player (make -C runtime), the one binary that has
// the module: sol test refuses to run it on the stock player.
test("the runtime's module answers", async () => {
  expect(hello()).toBe("Hello from the runtime")
})
