// flux:process: liveness and signal listeners.
import { expect, test } from "flux:test"
import { alive, on, once, pid, platform } from "flux:process"
import { command } from "flux:subprocess"

test("alive sees its own process and not a free pid", () => {
  expect(alive(pid)).toBe(true)
  expect(alive(4294967294)).toBe(false)
})

if (platform !== "win32") {
  test("a resubscribe in one tick keeps the watcher", async () => {
    // off() then on() in one tick must keep the OS watcher: the wakeup from
    // the unsubscribe finds a listener again and reads on, so the next
    // delivery still reaches JS.
    let off = on("SIGUSR1", () => {})
    off()
    let got = new Promise<string>((resolve) => once("SIGUSR1", resolve))
    await command("kill", ["-USR1", String(pid)]).output()
    expect(await got).toBe("SIGUSR1")
  })
}
