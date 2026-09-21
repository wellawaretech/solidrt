// The server's identity banner: how a client reaches this server, printed
// once at startup and again by the repl's `whoami`. One QR on screen: with
// the tunnel on, the ticket QR is the pairing story and the address stays
// text-only; on the LAN without it, the address QR is the scan target.
// Loopback-only has nothing to scan.

import { state } from "./state"
import { printQr } from "./qr"

export function printIdentity() {
  let { config, tunnelTicket, serverUrl } = state
  if (tunnelTicket !== null) {
    console.log("")
    printQr(tunnelTicket)
    console.log("")
    console.log(`[cli] Tunnel ticket: ${tunnelTicket}`)
  } else if (config.lan) {
    console.log("")
    printQr(serverUrl)
    console.log("")
  }
  console.log(`[cli] Dev server on http://${serverUrl} serving ${config.mode} ${config.key}`)
}
