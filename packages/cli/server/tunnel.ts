// The p2p tunnel: an iroh endpoint carrying the dev protocol for clients that
// pair by ticket instead of dialing the TCP port. One accepted bi-stream
// carries one TCP connection (like `ssh -L`): bytes are pumped verbatim into a
// loopback connection on the serve port, so the HTTP/WS protocol rides through
// unchanged. Local-only: the endpoint uses no relay and publishes nothing, so
// the ticket (direct addresses) is the sole carrier of addressing. Off-LAN
// relay support is a future opt-in flag, not the default.

import { Endpoint, type P2pStream } from "flux:p2p"
import { connect, type Conn } from "flux:net"
import { printQr } from "./qr"

// The tunnel's ALPN. A protocol change bumps the suffix so old clients fail
// the handshake instead of desyncing.
const PROTOCOL = "solidrt-dev/0"

/**
 * Bind the tunnel endpoint, print its ticket (text + QR), and serve incoming
 * streams until the process exits. The key is ephemeral on purpose: a ticket
 * embeds the endpoint's bound port, which changes every run anyway, so
 * persisting the key would not make the ticket stable.
 */
export async function startTunnel(port: number) {
  let endpoint = await Endpoint.create({ local: true, protocols: [PROTOCOL] })
  let ticket = await endpoint.ticket()
  console.log("")
  printQr(ticket)
  console.log("")
  console.log(`[cli] Tunnel ticket: ${ticket}`)

  for await (let stream of endpoint.accept(PROTOCOL)) {
    serveStream(stream, port)
  }
}

// Pump one stream <-> one loopback TCP connection until either side closes.
async function serveStream(stream: P2pStream, port: number) {
  console.log(`[cli] Tunnel stream from ${stream.remoteId.slice(0, 8)}`)
  let conn: Conn
  try {
    conn = await connect("127.0.0.1", port)
  } catch (e) {
    console.log(`[cli] Tunnel loopback connect failed: ${e}`)
    stream.close()
    return
  }
  let up = (async () => {
    for await (let chunk of stream) await conn.write(chunk)
  })()
  let down = (async () => {
    for await (let chunk of conn) stream.write(chunk)
  })()
  // Either direction ending (or failing) tears the pair down; the other loop
  // then finishes on its closed end.
  try {
    await Promise.race([up, down])
  } catch {}
  conn.close()
  stream.close()
  await Promise.allSettled([up, down])
}
