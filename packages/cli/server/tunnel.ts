// The p2p tunnel: an iroh endpoint carrying the dev protocol for clients that
// pair by ticket instead of dialing the TCP port. serve() accepts connections
// on the endpoint directly (the endpoint/protocol serve options), so the
// HTTP/WS protocol is spoken straight over each connection's first bi-stream -
// no pump. Local-only: the endpoint uses no relay and publishes nothing, so
// the ticket (direct addresses) is the sole carrier of addressing. Off-LAN
// relay support is a future opt-in flag, not the default.

import { Endpoint } from "flux:p2p"
import { printQr } from "./qr"

// The tunnel's ALPN. A protocol change bumps the suffix so old clients fail
// the handshake instead of desyncing.
export const TUNNEL_PROTOCOL = "solidrt-dev/0"

/**
 * Bind the tunnel endpoint and print its ticket (text + QR). The key is
 * ephemeral on purpose: a ticket embeds the endpoint's bound port, which
 * changes every run anyway, so persisting the key would not make the ticket
 * stable.
 */
export async function createTunnelEndpoint(): Promise<Endpoint> {
  let endpoint = await Endpoint.create({ local: true, protocols: [TUNNEL_PROTOCOL] })
  let ticket = await endpoint.ticket()
  console.log("")
  printQr(ticket)
  console.log("")
  console.log(`[cli] Tunnel ticket: ${ticket}`)
  return endpoint
}
