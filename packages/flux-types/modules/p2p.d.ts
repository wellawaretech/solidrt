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
    /** A self-hosted relay URL. Omit for no relay: direct paths only. */
    relayUrl?: string
    /** Protocols this endpoint will {@link Endpoint.accept}. */
    protocols?: string[]
    /**
     * Pin the UDP bind port (IPv4-only). With a persisted `secretKey` this keeps
     * the whole ticket stable across restarts, so a paired client can re-dial
     * the old ticket. Omit for an ephemeral port.
     */
    port?: number
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

  /** A bound iroh endpoint with a stable keypair. */
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
}