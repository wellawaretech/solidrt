declare module "flux:p2p" {
  /**
   * Options for {@link Endpoint.create}. No public infrastructure is used: no
   * public relay and no address publishing or lookup, so nothing about the
   * endpoint leaves the machine except its {@link Endpoint.ticket}, which
   * peers dial (without a relay its default-route IPv4 reaches
   * same-network peers; a `relayUrl` adds a path beyond the local network,
   * and the ticket then names only the relay, see
   * {@link TicketOptions.addresses}). Every endpoint binds one IPv4 socket: no
   * IPv6 path is dialled or advertised (a phone's rotating global IPv6
   * addresses stalled streams under a connection, and the local network
   * every device shares is IPv4).
   */
  type EndpointOptions = {
    /**
     * 64 hex chars (32 bytes) for a stable identity across restarts. Omit for an
     * ephemeral key.
     */
    secretKey?: string
    /**
     * A self-hosted relay URL ({@link Relay.url} of a {@link serveRelay} in
     * some process on the network, or a relay the app operates), with the
     * relay's token as its user part when it has one
     * (`http://<token>@host:port/`, what `serveRelay` hands out and a ticket
     * carries). The endpoint connects to it, its ticket carries it, and a
     * peer with no direct UDP path (or one that fails) is reached through
     * it. Omit for no relay of its own: direct paths, plus the relay of a
     * ticket it {@link Endpoint.connect}s to. Never a public relay.
     * {@link Endpoint.setRelayUrl} changes it later.
     *
     * An `https://` relay is checked by its certificate, in one of two ways:
     *
     * - By its pin, `?pin=<64 hex>` in the URL
     *   (`https://<token>@host:port/?pin=<hex>`, what an HTTPS `serveRelay`
     *   with a self-signed certificate hands out). The pin is the SHA-256
     *   of the relay's public key, the `pin` of `generateCertificate`
     *   (flux:net). It replaces the chain and the name check; the handshake
     *   signature is still verified. It travels in the URL so that the URL
     *   stays the one string that configures a client, and a ticket carries
     *   it unchanged.
     * - Without a pin, by its chain: it must end at a web root or at one of
     *   {@link relayCaCerts}, and the certificate must name the URL's host.
     *
     * The token and the pin are removed from the URL the endpoint dials,
     * and come back in its ticket. {@link Endpoint.create} rejects a
     * scheme other than `http://` and `https://` (such as `wss://`), a pin
     * that is not 64 hex characters, a pin on `http://` (no certificate to
     * check), a pin given twice, any other query parameter, a password in
     * the user part (the token is the whole user part), a fragment, a `|`
     * anywhere (it separates a ticket's fields), and an `https://` URL with
     * an IPv6 address (the relay client cannot check its certificate; see
     * {@link RelayOptions.host}). An `http://` URL with an IPv6 address is
     * fine. The rejection does not repeat the URL, whose user part is the
     * secret. A ticket's relay is checked the same way.
     */
    relayUrl?: string
    /**
     * Extra trust anchors for the certificates of HTTPS relays without a
     * pin, on top of the web roots: PEM, each string possibly several
     * certificates (a private CA). They apply to `relayUrl` and to every
     * relay adopted by {@link Endpoint.connect}; a pinned relay is checked
     * by its pin only. {@link Endpoint.create} rejects a string that is not
     * a PEM certificate.
     */
    relayCaCerts?: string[]
    /** Protocols this endpoint will {@link Endpoint.accept}. */
    protocols?: string[]
    /**
     * Pin the UDP bind port (IPv4-only). With a persisted `secretKey` this keeps
     * the whole ticket stable across restarts, so a paired client can re-dial
     * the old ticket. Omit for an ephemeral port.
     */
    port?: number
    /**
     * Which paths the endpoint uses: "any" (the default: a direct IPv4 path,
     * the relay when one is named) or "relay" (no UDP socket at all, every
     * packet through the `relayUrl`, which is then required). "relay" is for
     * a device whose UDP sends fail, which today is a phone on a noq-udp
     * version with the EIO fallback bug: its direct path dies in both
     * directions while it still looks alive to it.
     */
    paths?: "any" | "relay"
  }

  /** One transport address from {@link Endpoint.connInfo}. */
  type ConnAddr = {
    /** "relay", "direct" (an IP path), or "custom". */
    kind: "relay" | "direct" | "custom"
    /** The address string. */
    addr: string
    /** Whether this path is currently active. */
    active: boolean
  }

  /** The state of an endpoint's `relayUrl`; see {@link Endpoint.relayStatus}. */
  type RelayStatus = {
    /**
     * The relay URL without its token and without its `?pin=` (the token is
     * the relay's whole secret, and this value lands in logs and error
     * text).
     */
    url: string
    /** Whether the endpoint is connected to the relay now. */
    connected: boolean
    /**
     * The most recent failure while not connected, e.g. a refused token.
     * Absent while connected, before a failure, and while the relay has not
     * answered at all. A certificate that fails the check gives its
     * reason here, e.g. "invalid peer certificate: the server's key does
     * not match the pin" for a wrong pin, or "invalid peer certificate:
     * UnknownIssuer" for a chain to no trusted root: the endpoint never
     * connects to such a relay. The reason is kept per host, and a later
     * certificate of that host that passes the check clears it. When iroh
     * made another relay the home relay (one adopted by
     * {@link Endpoint.connect}, or the relay that
     * {@link Endpoint.setRelayUrl} replaced, until the new one answers), it
     * reads "iroh made <url> the home relay", the url without its token
     * and pin.
     */
    error?: string
  }

  /** A relay an endpoint knows; see {@link Endpoint.relays}. */
  type KnownRelay = {
    /** The relay URL without its token and without its `?pin=`. */
    url: string
    /** The pin the relay is checked by (64 hex), absent for none. */
    pin?: string
    /**
     * `true` for the endpoint's `relayUrl` (as created, or as set by
     * {@link Endpoint.setRelayUrl}), `false` for a relay adopted by
     * {@link Endpoint.connect}.
     */
    configured: boolean
    /**
     * Whether iroh made this relay the home relay now (iroh keeps one, see
     * {@link Endpoint.connect}), connected or still connecting.
     */
    home: boolean
  }

  /** Options for {@link Endpoint.ticket}. */
  type TicketOptions = {
    /**
     * Write the endpoint's `relayUrl` into the ticket, and wait for it.
     * Default `true`. With `false` the ticket is direct-only (no relay):
     * it waits only for a direct address and never rejects for the relay.
     * An endpoint with `paths: "relay"` has no direct address, so `false`
     * rejects at once there.
     */
    relay?: boolean
    /**
     * Which direct addresses the ticket carries. A ticket is a string made
     * to be passed on (a QR code, a link, a chat message), so by default it
     * carries only what a peer needs, not the machine's network layout (a
     * VPN, a container bridge, the LAN subnet, a global IPv6 address).
     *
     * - `"auto"` (the default): a ticket that names a relay carries no
     *   address. The peers meet on the relay, exchange their addresses
     *   inside the encrypted connection, and then move to a direct path
     *   when one exists (see {@link Endpoint.connInfo}); the first packets
     *   go through the relay. The relay is then a hard dependency: a peer
     *   that cannot reach it cannot connect, even on the same network.
     *   A relay whose host is `localhost` (or a name under it) or a
     *   loopback, link-local or unspecified IP address (a `serveRelay({ host: "127.0.0.1" })`)
     *   serves this machine only, so beside it the ticket carries what a
     *   ticket without a relay does. A ticket without a relay (no
     *   `relayUrl`, or `relay: false`) carries the IPv4 address of the
     *   interface that has the default route: the address a peer on the
     *   same network uses. With no default route, or none of its IPv4
     *   addresses among the endpoint's, it carries every IPv4 address.
     *   Never an IPv6 address. A full-tunnel VPN may own the default
     *   route, and the ticket then carries the VPN address (Linux
     *   wg-quick and Tailscale use policy routing, so the main default
     *   route usually stays on the LAN); with two networks (Ethernet and
     *   Wi-Fi) only the one of the default route is in it.
     * - `"all"`: every direct address of the endpoint, also beside a relay:
     *   the opt-out of the relay dependency, since a peer that cannot reach
     *   the relay can still dial one of them. It also carries a portmapped
     *   (UPnP, PCP) public address when the router gave one.
     *
     * Any other value throws. {@link parseTicket} shows what a ticket
     * carries.
     */
    addresses?: "auto" | "all"
  }

  /** Options for {@link Endpoint.connect}. */
  type ConnectOptions = {
    /**
     * Adopt the ticket's relay when this endpoint does not know it yet.
     * Default `true`. With `false` an unknown ticket relay is not added and
     * not dialled: the dial uses the ticket's relay only when this endpoint
     * knows it already, and the ticket's direct addresses. iroh never
     * routes to a peer through this endpoint's own relay unless the ticket
     * names it, so a ticket whose relay is unknown and that carries no
     * direct address rejects before the dial ("Relay <url>: not known to
     * this endpoint and adoptRelay is false, and the ticket carries no
     * direct address", the url without its token and pin). A ticket relay
     * the endpoint knows is used either way.
     */
    adoptRelay?: boolean
  }

  /** A snapshot of how a connection is currently carried. */
  type ConnInfo = {
    /**
     * "direct" (a direct IP path is active), "relay" (only a relay path),
     * "mixed" (both), or "none".
     */
    path: "direct" | "relay" | "mixed" | "none"
    /** Every known transport address. */
    addrs: ConnAddr[]
  }

  /**
   * A single bidirectional p2p stream: a byte duplex as a `readable`/`writable`
   * pair of web streams, the same shape as a `flux:net` `Conn`, so
   * `stream.readable.pipeTo(conn.writable)` bridges the two.
   */
  export class P2pStream {
    /** The remote peer's endpoint id. */
    readonly remoteId: string
    /**
     * What the peer sends, one chunk per read, pulled only as far as it is
     * read. Ends when the peer finishes its send half or closes, or on
     * {@link close}; errors when the peer resets the stream, and with the
     * reason after `writable.abort(reason)`. `cancel()` stops reading and
     * releases the recv half, which tells the peer to stop sending (its
     * writes reject); the send side stays usable until {@link close}.
     */
    readonly readable: ReadableStream<Uint8Array>
    /**
     * What to send: strings (as UTF-8) or `Uint8Array`s. A write resolves
     * once the transport took the bytes, paced by QUIC flow control, so a
     * producer faster than the peer reads sees backpressure; a write to a
     * peer that went away or stopped reading rejects. `close()` finishes the
     * send half (QUIC FIN): the peer sees end-of-stream, the recv half stays
     * open for replies, and a later write rejects. `abort(reason)` resets the
     * send half and tears the stream down with an error: the peer's read
     * fails instead of ending, and a pending or later read of
     * {@link readable} here rejects with the reason.
     */
    readonly writable: WritableStream<string | Uint8Array>
    /**
     * Tear the stream down: a pending read ends, the recv half is released and
     * the send half finished.
     */
    close(): void
  }

  /**
   * A bound iroh endpoint with a stable keypair. Only in a build with the
   * "p2p" capability (the stock runtimes have it); the export is absent
   * without it.
   */
  export class Endpoint {
    /**
     * Bind an endpoint.
     *
     * @param opts  secretKey, relayUrl, relayCaCerts, protocols, port, paths.
     */
    static create(opts?: EndpointOptions): Promise<Endpoint>
    /** This endpoint's identity (its public key), as {@link P2pStream.remoteId} reports it to peers. */
    readonly id: string
    /** The secret key as 64 hex chars, for the caller to persist and feed back to {@link create}. */
    readonly secretKey: string
    /**
     * A self-contained dial token (`id|relay|ips`): what a peer passes to
     * {@link connect}. The relay is this endpoint's `relayUrl` (with its
     * token and pin), never one adopted by {@link connect}; empty without
     * one, or with `relay: false`. The direct addresses follow
     * {@link TicketOptions.addresses}: by default none beside a relay
     * (which makes the relay the only way in: use `addresses: "all"` for a
     * peer that may not reach it), the local ones beside a relay that
     * serves this machine only, else the default-route IPv4. It waits up
     * to 3 s for what makes it dialable: the endpoint connected to its own
     * `relayUrl` when it writes one (another relay connected does not
     * count), else a direct address.
     * With a `relayUrl` that is still not connected then, it rejects with
     * an error that names the relay (without its token and pin) and the
     * reason (see {@link relayStatus}): that iroh made another relay the
     * home relay (one adopted by {@link connect}), a refused token or
     * endpoint id, a certificate that fails the check (a wrong pin, an
     * unknown issuer), or a relay that did not answer. A ticket naming a
     * relay that is down would fail only later, on the peer's side; use
     * `relay: false` for a direct-only ticket (a peer on the same network)
     * instead. An endpoint with `paths: "relay"` has no direct address, so
     * `relay: false` rejects there at once: that ticket could not be dialed.
     * The relay is the `relayUrl` when `ticket` is called: a call already
     * waiting when {@link setRelayUrl} switches names the relay it started
     * with.
     */
    ticket(opts?: TicketOptions): Promise<string>
    /**
     * The state of this endpoint's `relayUrl`, or `null` without one. A
     * relay that has not answered yet (or never does) reads as not
     * connected, with no `error`. A relay adopted by {@link connect} is not
     * reported; one that iroh made the home relay leaves `relayUrl` not
     * connected (see {@link connect}). After {@link setRelayUrl} it reports
     * the new relay, and until that relay answers, the error "iroh made
     * <old url> the home relay".
     */
    relayStatus(): RelayStatus | null
    /**
     * The relays this endpoint knows: its `relayUrl` first, then the relays
     * {@link connect} adopted, in adoption order. Only {@link setRelayUrl}
     * removes one: the relay it replaces.
     */
    relays(): KnownRelay[]
    /**
     * Make `url` this endpoint's relay, in place: open connections and
     * streams keep running. From then on {@link ticket} and
     * {@link relayStatus} name it, and {@link relays} lists it first. The
     * URL takes the form of {@link EndpointOptions.relayUrl} (the token as
     * the user part, `?pin=`) and rejects as {@link create} does. It
     * resolves once the switch is made, not when the relay connects:
     * {@link ticket} waits for that, and {@link relayStatus} reports it. A
     * use: an endpoint without a relay makes a scanned ticket's relay its
     * own, `setRelayUrl(parseTicket(ticket).relayUrl)`.
     *
     * A relay this endpoint already knows (its `relayUrl`, one adopted by
     * {@link connect}, one that `setRelayUrl` replaced before) keeps the
     * token and pin it was first used with: the connection to it already
     * runs with them. A URL with another token or pin rejects, naming the
     * relay (without its token and pin) and what differs ("Relay <url>:
     * the URL carries another token than this endpoint uses for it; ...").
     * A URL without a token or without a pin uses the known one. So a new
     * token or key on one URL needs a new endpoint. The current `relayUrl`
     * with the same token and pin does nothing. The certificate check is
     * per host, as for {@link connect}: a relay on a host this endpoint
     * checks another way rejects, and the endpoint stays as it was.
     *
     * The old relay stays the home relay until the new one answers:
     * {@link relayStatus} reports the new relay not connected then, with
     * "iroh made <old url> the home relay". A connection through the old
     * relay keeps running. Tickets handed out before the switch name the
     * old relay, and stop reaching this endpoint once no connection uses
     * it any more: hand out new ones. The gap of {@link connect} stays:
     * iroh may still make a relay adopted by {@link connect} the home
     * relay instead of the new one.
     *
     * Only a string is taken: anything else throws. An endpoint cannot drop
     * its relay (iroh never lets go of a home relay); use
     * `ticket({ relay: false })` to stop naming it.
     */
    setRelayUrl(url: string): Promise<void>
    /**
     * Dial a peer and open one bidirectional stream over `protocol`. `peer` is
     * its {@link ticket}. A bare endpoint `id` is accepted too, but with no
     * address lookup it only reaches a peer this endpoint already has a path
     * to (one it is connected to). The peer sees the stream once
     * this side writes to it (see {@link accept}).
     *
     * A ticket's relay this endpoint does not know yet is adopted: added
     * with the ticket's token and pin, then dialled through, so an endpoint
     * with no `relayUrl` (or another one) reaches a peer on the peer's own
     * relay. `adoptRelay: false` turns this off for one connect. An adopted
     * relay stays until {@link close} ({@link relays} lists it), unless
     * {@link setRelayUrl} makes it the `relayUrl`. It may carry this
     * endpoint's traffic, but {@link ticket} and {@link relayStatus} still
     * name only `relayUrl`. To make the ticket's relay the endpoint's own,
     * read it with {@link parseTicket} and pass it to {@link setRelayUrl}.
     * A relay that {@link setRelayUrl} replaced is adopted again with the
     * token and pin it was first used with (with `adoptRelay: false` it is
     * unknown).
     *
     * A relay the endpoint already knows (its `relayUrl`, or one adopted
     * before) keeps the token and pin it was first used with: the
     * connection to it already runs with them. A ticket that names it with
     * another pin rejects, and so does a pin for a relay this endpoint
     * checks by its chain; for an adopted relay, so does another token, or
     * any token when it was adopted without one. The error names the relay
     * (without its token and pin) and what differs. So a relay restarted
     * with a new token or key needs a new endpoint to reach it by a new
     * ticket. Another token for the endpoint's own `relayUrl` is fine (the
     * relay admits the endpoint's own token). A ticket without a token or
     * without a pin uses the known one: a pinned relay is still checked by
     * its pin.
     *
     * The certificate check is per host, not per port: one host, one check
     * (one pin, or the chain). A ticket relay on a host this endpoint
     * already checks another way (another pin, or a pin where the chain is
     * checked, or the other way round) rejects the connect, naming the
     * relay; it is not adopted. Give two relays on one host the same
     * certificate, or reach them by different host names.
     *
     * One gap: an endpoint with its own `relayUrl` that connects through a
     * different relay may have iroh move its home relay there (iroh keeps
     * one home relay, the fastest). Its own relay then drops when idle, its
     * tickets (which still name `relayUrl`) may not reach it, and
     * {@link relayStatus} reports it not connected. So an app with its own
     * relay shares that relay with its peers, or dials a ticket that names
     * another relay from a separate endpoint created with that relay
     * ({@link parseTicket}). An endpoint without a `relayUrl` has no such
     * gap.
     */
    connect(peer: string, protocol: string, opts?: ConnectOptions): Promise<P2pStream>
    /**
     * An async-iterable of incoming streams whose protocol matches `protocol`.
     * A dialled stream arrives here with its first bytes (a QUIC stream is
     * announced by its data), so the dialer writes first and the acceptor
     * reads first: two sides that both wait for the other never meet.
     * Iterating ends when the endpoint is closed.
     */
    accept(protocol: string): AsyncIterable<P2pStream>
    /** Snapshot of how the connection to `id` is currently carried. */
    connInfo(id: string): Promise<ConnInfo>
    /** Close the endpoint, ending any {@link accept} iteration. */
    close(): Promise<void>
  }

  /** The parts of a ticket; see {@link parseTicket}. */
  type Ticket = {
    /** The peer's endpoint id. */
    id: string
    /**
     * The relay URL with its token as the user part and its `?pin=`, as
     * {@link EndpointOptions.relayUrl} takes it; `null` when the ticket
     * names no relay.
     */
    relayUrl: string | null
    /** The direct addresses, `ip:port` each. */
    addresses: string[]
  }

  /**
   * Split a {@link Endpoint.ticket} (or a bare endpoint id) into its parts,
   * as {@link Endpoint.connect} reads it. Throws when it does not parse: an
   * invalid id, relay URL (see {@link EndpointOptions.relayUrl}) or
   * address, or more than three `|`-separated fields. Only in a build with
   * the "p2p" capability.
   */
  export function parseTicket(ticket: string): Ticket

  /** Options for {@link serveRelay}. */
  type RelayOptions = {
    /**
     * The TCP port to listen on: HTTPS with `tls`, else plain HTTP. Omit
     * (or 0) for a free one.
     */
    port?: number
    /**
     * The bind address, an IP address. Default "0.0.0.0" (every IPv4
     * interface). "127.0.0.1" keeps the relay behind a local TLS proxy
     * (with `url`). Throws when it is not an IP address. A specific IPv6
     * address with `tls` (and no `url`) throws too: the relay client cannot
     * check the certificate of an HTTPS relay URL with an IPv6 address, so
     * no endpoint could reach it. Give such a relay a `url` with a DNS
     * name. The unspecified "::" does not throw: its URL carries the IPv4
     * of the default route (see {@link Relay.url}).
     */
    host?: string
    /**
     * HTTPS: the certificate chain and its private key (PEM), as
     * `generateCertificate` of flux:net makes them or any CA issues them.
     * Its result passes as it is (`tls: generateCertificate(...)`): its
     * `pin` string is not read. `caCerts` and `anyClientCert` throw: relay
     * clients present no certificate.
     *
     * `pinned` decides whether {@link Relay.url} carries the leaf's pin
     * (see {@link EndpointOptions.relayUrl}). By default it does for a
     * self-signed leaf (its issuer equals its subject) and does not for a
     * CA-signed one, which clients check by its chain. A CA certificate
     * whose subject equals its leaf's subject reads as self-signed: set
     * `pinned: false` for it.
     *
     * The pin stays the same only as long as the key does: save the
     * `cert` and `key` once and pass the same ones on every start (as an
     * endpoint's `secretKey`). A new key changes the pin, and so every URL,
     * ticket and config that holds it. `generateCertificate` makes 10-year
     * certificates by default.
     *
     * A CA-signed certificate comes from the app's own ACME client (such as
     * certbot), or the relay sits behind a reverse proxy that terminates
     * TLS: then no `tls`, `host: "127.0.0.1"`, and `url` the proxy's public
     * `https://` URL. The runtime has no built-in ACME. Renew the
     * certificate with {@link Relay.setCertificate}, with no restart.
     */
    tls?: { cert: string; key: string; pinned?: boolean }
    /**
     * The tokens the relay admits; the first goes into {@link Relay.url}.
     * Default: one fresh random token per run. A fixed token keeps the URL
     * (and so every ticket and config that holds it) stable across
     * restarts; a second one lets the old URL work while clients move to
     * the new one. Each token is at least 16 characters of letters, digits
     * and `- . _ ~`; anything else, or an empty list, throws.
     */
    token?: string | string[]
    /**
     * The endpoint ids the relay admits, checked after the token. Omit for
     * any endpoint with a token; an empty list admits none. Throws on a
     * malformed id. See {@link Relay.setAllow}.
     */
    allow?: string[]
    /**
     * The public base URL, when peers reach the relay by another address
     * than the bound one (a proxy, a NAT, a DNS name): `http://` or
     * `https://` with a host and no user part, query, fragment or `|`
     * (those throw). {@link Relay.url} is it with the token (and the pin)
     * added. An `http://` url with `tls` throws, pinned or not (clients
     * would dial it without TLS), and so does an `https://` url with an
     * IPv6 address (see {@link host}).
     */
    url?: string
  }

  /** A relay this process serves; see {@link serveRelay}. */
  export class Relay {
    /** The port the relay listens on (the HTTPS one with `tls`). */
    readonly port: number
    /**
     * The relay URL peers pass as `relayUrl`, in the form a ticket carries:
     * `http://<token>@host:port/` without `tls`, `https://<token>@host:port/`
     * with it, plus `?pin=<hex>` when the certificate is pinned (see
     * {@link RelayOptions.tls}). The token is the first of
     * {@link RelayOptions.token}. The scheme, host and port are `url`'s
     * when set. Else the host is the bound `host`, or for "0.0.0.0" (and
     * "::") the IPv4 of this machine's default route (the address other
     * hosts on the network reach it by; without a default route the first
     * IPv4 that is neither loopback nor link-local, and the loopback
     * address only on a machine with none). The token is the secret:
     * whoever has the URL can use the relay, unless `allow` limits it.
     */
    readonly url: string
    /**
     * Replace the allowlist; `null` admits any endpoint with a token.
     * Connected endpoints the new list does not admit are disconnected,
     * and their reconnects are denied; connected endpoints it admits stay.
     * Throws on a malformed id, and when the argument is missing (so a
     * forgotten argument never opens the relay).
     */
    setAllow(ids: string[] | null): void
    /** The endpoint ids connected to the relay now. */
    clients(): string[]
    /**
     * Renew the certificate of an HTTPS relay while it serves: handshakes
     * from now on present the new `cert` and `key` (PEM, as in
     * {@link RelayOptions.tls}), and the clients connected before stay
     * connected. A `generateCertificate` result passes as it is.
     *
     * {@link url} never changes. So a pinned relay takes only a certificate
     * on the same key: a new key changes the pin that every ticket, QR code
     * and endpoint holds, and an endpoint cannot take a new pin for a relay
     * it knows. For a new key, close the relay and serve a new one, with a
     * new URL. An unpinned relay takes any key (clients check its chain):
     * this is the Let's Encrypt renewal, which makes a new key each time.
     * `pinned` stays as the relay started (passing it throws).
     *
     * Throws, keeping the current certificate, on another key for a pinned
     * relay, when either is not PEM, and when the key is not the
     * certificate's. Throws on a relay without `tls`.
     */
    setCertificate(tls: { cert: string; key: string }): void
    /** Stop the relay: its listener closes and the connections it carries end. */
    close(): Promise<void>
  }

  /**
   * Serve an iroh relay from this process, for the endpoints that name its
   * {@link Relay.url} as `relayUrl`: two peers with no direct UDP path to each
   * other (or whose direct path fails) exchange their packets through it,
   * each over a TCP connection to the relay, while iroh keeps probing for a
   * direct path. Plain HTTP on the network the devices share by default,
   * HTTPS with {@link RelayOptions.tls}; everything carried is end-to-end
   * encrypted QUIC, so the relay reads nothing. HTTPS protects the hop to
   * the relay: on a public network plain HTTP shows the token and which
   * endpoints relay to which. The relay admits only endpoints that present
   * one of its {@link RelayOptions.token}s, carried in {@link Relay.url},
   * and, with {@link RelayOptions.allow}, only the listed endpoint ids; a
   * reachable port is not an open relay. An invalid option throws. The
   * relay runs until {@link Relay.close} or the end of the program. Only
   * in a build with the "p2p-relay" capability (the stock runtimes have
   * it); the export is absent without it.
   */
  export function serveRelay(opts?: RelayOptions): Promise<Relay>
}