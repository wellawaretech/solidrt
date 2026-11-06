// flux:mdns. Full end-to-end resolution of `.local` names needs a LAN with
// a Bonjour/avahi responder, which a test machine may lack, so these assert
// the deterministic no-responder paths: the surface exists, the queries
// resolve to arrays (empty here), and nothing crashes. The query timeoutMs
// is kept short so the no-answer window closes fast.
//
// The network queries are registered off macOS only. On the macos-15 CI
// runner the multicast send fails with "No route to host (os error 65)",
// most likely macOS Local Network privacy denying the runner agent, which a
// script cannot grant. The same queries pass on a Mac from an ssh shell,
// which that privacy check does not cover.
import { expect, test } from "flux:test"
import { browse, resolve, services } from "flux:mdns"
import { platform } from "flux:process"

test("resolve of an empty input is empty", async () => {
  // No addresses to resolve: resolves to [] without touching the network.
  expect(await resolve([])).toEqual([])
})

if (platform !== "darwin") {
  test("browse of an unknown service resolves to an array", async () => {
    // A service nobody advertises yields an empty array within the timeout
    // window.
    expect(Array.isArray(await browse("_nonexistent._tcp", { timeoutMs: 300 }))).toBe(true)
  })

  test("services resolves to an array", async () => {
    // The service-enumeration query resolves to an array (possibly empty
    // here).
    expect(Array.isArray(await services({ timeoutMs: 300 }))).toBe(true)
  })
}
