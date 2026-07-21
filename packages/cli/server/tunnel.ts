// The p2p tunnel: an iroh endpoint carrying the dev protocol for clients that
// pair by ticket instead of dialing the TCP port. serve() accepts connections
// on the endpoint directly (the endpoint/protocol serve options), so the
// HTTP/WS protocol is spoken straight over each connection's first bi-stream -
// no pump. Local-only: the endpoint uses no relay and publishes nothing, so
// the ticket (direct addresses) is the sole carrier of addressing. Off-LAN
// relay support is a future opt-in flag, not the default.

import { Endpoint } from "flux:p2p"
import { file } from "flux:fs"
import { join } from "flux:path"
import { printQr } from "./qr"

// The tunnel's ALPN. A protocol change bumps the suffix so old clients fail
// the handshake instead of desyncing.
export const TUNNEL_PROTOCOL = "solidrt-dev/0"

// The persisted identity file, at the project root. Delete it to rotate the
// tunnel's identity (which invalidates any old ticket).
const KEY_FILE = ".srt-tunnel-key"

/**
 * Bind the tunnel endpoint and print its ticket (text + QR). The endpoint is
 * kept stable across restarts so a paired client can re-dial the old ticket
 * without re-scanning: the UDP port is pinned to the dev server's port, and the
 * secret key is persisted in <keyDir>/.srt-tunnel-key (generated on first
 * run). Both are needed - a moving port or a fresh key each start would change
 * the ticket. Stable across restarts on the same network only; a new machine IP
 * still stales the ticket's addresses (that is the discovery/off-LAN story).
 */
export async function createTunnelEndpoint(port: number, keyDir: string): Promise<Endpoint> {
  let keyPath = join(keyDir, KEY_FILE)

  let secretKey: string | undefined
  let keyFile = file(keyPath)
  if (await keyFile.exists()) {
    let saved = (await keyFile.text()).trim()
    if (saved.length === 64) secretKey = saved
  }

  let endpoint = await Endpoint.create({ local: true, protocols: [TUNNEL_PROTOCOL], port, secretKey })

  // First run (no saved key): persist the freshly generated one so the next run
  // reuses it and the ticket stays the same.
  if (!secretKey) await keyFile.write(endpoint.secretKey)

  let ticket = await endpoint.ticket()
  console.log("")
  printQr(ticket)
  console.log("")
  console.log(`[cli] Tunnel ticket: ${ticket}`)
  return endpoint
}
