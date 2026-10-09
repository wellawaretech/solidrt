// flux:subprocess: a spawned child's stdin as a WritableStream, fed by a pipe
// and read back from stdout. `cat` is the pipe; Windows has none.
import { expect, test } from "flux:test"
import { platform } from "flux:process"
import { command } from "flux:subprocess"

if (platform !== "win32") {
  test("a body piped into cat's stdin comes back from its stdout", async () => {
    let child = command("cat").spawn()
    expect(child.stdin instanceof WritableStream).toBe(true)
    // The pipe closes stdin at the body's end, so cat sees EOF and exits.
    await new Response("piped through cat").body.pipeTo(child.stdin)
    expect(await new Response(child.stdout).text()).toBe("piped through cat")
    expect((await child.status()).success).toBe(true)
  })

  test("the stdin option is written first, the stream after it", async () => {
    let child = command("cat", [], { stdin: "first, " }).spawn()
    let writer = child.stdin.getWriter()
    await writer.write("then ")
    await writer.write(new TextEncoder().encode("the stream"))
    await writer.close()
    expect(await new Response(child.stdout).text()).toBe("first, then the stream")
    await expect(writer.write("late")).rejects.toThrow("closing or closed")
  })

  test("a detached child has no stdin: the first write rejects", async () => {
    let child = command("sleep", ["0"], { detached: true }).spawn()
    await expect(child.stdin.getWriter().write("x")).rejects.toThrow("stdin is closed")
    await child.status()
  })
}
