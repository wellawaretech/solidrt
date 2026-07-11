declare module "flux:p2p" {
  /** Options for {@link Endpoint.create}. */
  type EndpointOptions = {
    /**
     * 64 hex chars (32 bytes) for a stable identity across restarts. Omit for an
     * ephemeral key.
     */
    secretKey?: string
    /** A self-hosted relay URL. Omit to use the public n0 relays. */
    relayUrl?: string
    /** Protocols this endpoint will {@link Endpoint.accept}. */
    protocols?: string[]
    /**
     * Bind local-only: no relay and no address publishing/lookup, so nothing
     * about the endpoint leaves the machine except the ticket itself, whose
     * direct IPs same-network peers dial. Excludes `relayUrl`; a bare-id
     * `connect` cannot resolve a local endpoint (tickets only).
     */
    local?: boolean
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
   * A single bidirectional p2p stream: a byte duplex. It is its own async
   * iterator, so `for await (let chunk of stream)` reads the recv half.
   */
  export class P2pStream implements AsyncIterable<Uint8Array> {
    /** The remote peer's endpoint id. */
    readonly remoteId: string
    /** Queue bytes on the send half. */
    write(data: string | Uint8Array): void
    /** Finish the send half (QUIC FIN) after queued writes flush. The recv half stays open. */
    finish(): void
    /** Tear the stream down: finish the send half and stop reading. */
    close(): void
    [Symbol.asyncIterator](): AsyncIterator<Uint8Array>
  }

  /** A bound iroh endpoint with a stable keypair. */
  export class Endpoint {
    /**
     * Bind an endpoint.
     *
     * @param opts  secretKey, relayUrl, protocols, local.
     */
    static create(opts?: EndpointOptions): Promise<Endpoint>
    /** This endpoint's dial address: the string peers pass to {@link connect}. */
    readonly id: string
    /** The secret key as 64 hex chars, for the caller to persist and feed back to {@link create}. */
    readonly secretKey: string
    /**
     * A self-contained dial token (`id|relay|ips`) so a peer can {@link connect}
     * without relying on discovery.
     */
    ticket(): Promise<string>
    /**
     * Dial a peer and open one bidirectional stream over `protocol`. `peer` is
     * either a `ticket` (preferred; connects directly) or a bare endpoint `id`
     * (needs discovery to resolve the address).
     */
    connect(peer: string, protocol: string): Promise<P2pStream>
    /**
     * An async-iterable of incoming streams whose protocol matches `protocol`.
     * Iterating ends when the endpoint is closed.
     */
    accept(protocol: string): AsyncIterable<P2pStream>
    /** Snapshot of how the connection to `id` is currently carried. */
    connInfo(id: string): Promise<ConnInfo>
    /** Close the endpoint, ending any {@link accept} iteration. */
    close(): Promise<void>
  }
}