declare module "flux:p2p" {
  /**
   * Options for {@link Endpoint.create}. No public infrastructure is used: no
   * public relay and no address publishing or lookup, so nothing about the
   * endpoint leaves the machine except its {@link Endpoint.ticket}, which
   * peers dial (its direct IPs reach same-network peers; a `relayUrl` adds a
   * path beyond the local network). Every endpoint binds one IPv4 socket: no
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
     * it. Omit for no relay: direct paths only. Never a public relay.
     */
    relayUrl?: string
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
     * @param opts  secretKey, relayUrl, protocols, port.
     */
    static create(opts?: EndpointOptions): Promise<Endpoint>
    /** This endpoint's identity (its public key), as {@link P2pStream.remoteId} reports it to peers. */
    readonly id: string
    /** The secret key as 64 hex chars, for the caller to persist and feed back to {@link create}. */
    readonly secretKey: string
    /**
     * A self-contained dial token (`id|relay|ips`): what a peer passes to
     * {@link connect}.
     */
    ticket(): Promise<string>
    /**
     * Dial a peer and open one bidirectional stream over `protocol`. `peer` is
     * its {@link ticket}. A bare endpoint `id` is accepted too, but with no
     * address lookup it only reaches a peer this endpoint already has a path
     * to (one it is connected to). The peer sees the stream once
     * this side writes to it (see {@link accept}).
     */
    connect(peer: string, protocol: string): Promise<P2pStream>
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

  /** Options for {@link serveRelay}. */
  type RelayOptions = {
    /** The TCP port to listen on, on every IPv4 interface. Omit (or 0) for a free one. */
    port?: number
  }

  /** A relay this process serves; see {@link serveRelay}. */
  export class Relay {
    /** The port the relay listens on. */
    readonly port: number
    /**
     * The relay URL peers pass as `relayUrl`: plain `http://`, the relay's
     * token as the user part, the IPv4 of this machine's default route (the
     * address other hosts on the network reach it by; the loopback address
     * on a machine without one) and the port, in the form a ticket carries.
     * The token is the whole secret: whoever has the URL can use the relay.
     */
    readonly url: string
    /** Stop the relay: its listener closes and the connections it carries end. */
    close(): Promise<void>
  }

  /**
   * Serve an iroh relay from this process, for the endpoints that name its
   * {@link Relay.url} as `relayUrl`: two peers with no direct UDP path to each
   * other (or whose direct path fails) exchange their packets through it,
   * each over a TCP connection to the relay, while iroh keeps probing for a
   * direct path. Plain HTTP on the network the devices share, no
   * certificate; everything carried is end-to-end encrypted QUIC, so the
   * relay reads nothing. It admits only endpoints that present its token,
   * a fresh random one per relay, carried in {@link Relay.url}; a reachable
   * port is not an open relay. The relay runs until
   * {@link Relay.close} or the end of the program. Only in a build with the
   * "p2p-relay" capability (the stock runtimes have it); the export is
   * absent without it.
   */
  export function serveRelay(opts?: RelayOptions): Promise<Relay>
}