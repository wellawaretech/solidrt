declare module "flux:net" {
  /** Options for {@link probe} and {@link connect}. */
  type ConnectOptions = {
    /** Give up after this many ms. Default 1000 for {@link probe}, 10000 for {@link connect}. */
    timeoutMs?: number
  }

  /** Options for {@link listen}. */
  type ListenOptions = {
    /** Local address to bind. Default "0.0.0.0" (all interfaces). */
    host?: string
  }

  /**
   * How a TLS client verifies the server and identifies itself: the options
   * of {@link connectTls} and {@link startTls}. By default the server must
   * have a certificate that chains to the built-in web roots and names the
   * host.
   */
  type TlsClientOptions = {
    /**
     * Give up after this many ms. Default 10000. For {@link connectTls} it
     * covers the connect and the handshake together.
     */
    timeoutMs?: number
    /** Extra trusted roots (PEM), on top of the built-in web roots: a private CA. */
    caCerts?: string[]
    /** A client certificate chain (PEM), for mutual TLS. Needs {@link key}. */
    cert?: string
    /** The private key (PEM) of {@link cert}. */
    key?: string
    /**
     * Trust the server by its key alone: the SHA-256 of its
     * SubjectPublicKeyInfo, as 64 hex characters (the form
     * {@link Conn.peerKey} reports, so store what you saw at pairing). It
     * replaces the chain check and the name check, so a self-signed
     * certificate on any address passes when its key matches. Cannot be
     * combined with {@link caCerts}.
     */
    pin?: string
    /** The protocols to offer by ALPN, in preference order (e.g. `["h2", "http/1.1"]`). */
    alpn?: string[]
    /**
     * Session resumption (see {@link connectTls}). `true`, the default:
     * automatic, an earlier session of equal options to the same server
     * name is offered. `false`: opt out, a full handshake that checks the
     * server's certificate afresh, for a server that misbehaves on
     * resumption; the sessions other connections resume from stay as they
     * are. A {@link Conn}: resume that connection's session. It must be a
     * TLS client connection with equal options (`caCerts`, `cert` and
     * `key`, `pin`, `alpn`), or this throws before it connects, so an FTPS
     * data connection the server would refuse fails visibly. It gives no
     * stronger guarantee than equal options do: the session offered is one
     * of equal options to the same server name, not necessarily that
     * connection's, and the server still decides.
     */
    resume?: boolean | Conn
  }

  /** Options for {@link startTls} as the client, the default. */
  type StartTlsClientOptions = TlsClientOptions & {
    /** Upgrade as the client: the default. */
    isServer?: false
    /**
     * The server name to verify and to send as SNI: the connection knows
     * only the peer's IP. Required unless {@link TlsClientOptions.pin} is
     * given.
     */
    hostname?: string
  }

  /**
   * Options for {@link startTls} as the server: the options of
   * {@link listenTls} without `host`, plus `timeoutMs`. `hostname`, `pin`
   * and `resume` are client options and throw here.
   */
  type StartTlsServerOptions = {
    /** Upgrade as the server (Node's `isServer`). */
    isServer: true
    /** Give up the handshake after this many ms. Default 10000. */
    timeoutMs?: number
    /** The server certificate chain (PEM). */
    cert: string
    /** The private key (PEM) of {@link cert}. */
    key: string
    /** The protocols to accept by ALPN, in preference order. */
    alpn?: string[]
    /**
     * Require a client certificate that chains to one of these roots (PEM).
     * Without it and without {@link anyClientCert}, no client certificate
     * is asked for.
     */
    caCerts?: string[]
    /**
     * Require a client certificate but accept any key: check
     * {@link Conn.peerKey} yourself (the pairing case). Cannot be combined
     * with {@link caCerts}.
     */
    anyClientCert?: boolean
  }

  /** Options for {@link startTls}: the client form, or the server form with `isServer: true`. */
  type StartTlsOptions = StartTlsClientOptions | StartTlsServerOptions

  /** Options for {@link generateCertificate}. */
  type GenerateCertificateOptions = {
    /**
     * The subject alternative names, DNS names and IP addresses alike
     * (`["localhost", "192.168.2.37"]`); at least one. A client verifies the
     * name it connects as against them. A DNS name may start with a `*.`
     * wildcard label; anything that is neither a DNS name nor an IP address
     * throws. The first is also the subject's common name.
     */
    names: string[]
    /**
     * How many days the certificate is valid, a positive integer. Default
     * 3650 (10 years). It is valid from an hour before it was made, so a
     * device whose clock runs a little behind accepts it at once.
     */
    validDays?: number
    /** Make a CA, which can be the {@link issuer} of other certificates. Default false. */
    isCa?: boolean
    /**
     * The CA that signs the certificate: its certificate and its PKCS#8
     * private key (PEM), as {@link generateCertificate} made them with
     * `isCa`. Without it the certificate is self-signed.
     */
    issuer?: { cert: string; key: string }
  }

  /** What {@link generateCertificate} makes. */
  type GeneratedCertificate = {
    /** The certificate (PEM). */
    cert: string
    /** Its private key (PKCS#8 PEM). Keep it secret. */
    key: string
    /**
     * The key's pin: the SHA-256 of its SubjectPublicKeyInfo as 64
     * lowercase hex characters, the form of {@link Conn.peerKey} and
     * {@link TlsClientOptions.pin}.
     */
    pin: string
  }

  /** Options for {@link listenTls}. */
  type ListenTlsOptions = {
    /** Local address to bind. Default "0.0.0.0" (all interfaces). */
    host?: string
    /** The server certificate chain (PEM). */
    cert: string
    /** The private key (PEM) of {@link cert}. */
    key: string
    /** The protocols to accept by ALPN, in preference order. */
    alpn?: string[]
    /**
     * Require a client certificate that chains to one of these roots (PEM).
     * Without it and without {@link anyClientCert}, no client certificate
     * is asked for.
     */
    caCerts?: string[]
    /**
     * Require a client certificate but accept any key: check
     * {@link Conn.peerKey} yourself (the pairing case). Cannot be combined
     * with {@link caCerts}.
     */
    anyClientCert?: boolean
  }

  /** Options for {@link udp}. */
  type UdpOptions = {
    /** Local port to bind. Default 0 (OS-assigned). */
    port?: number
    /** Set SO_REUSEADDR/REUSEPORT so several sockets can share the port. */
    reuse?: boolean
  }

  /**
   * Outcome of a {@link probe}. `closed` (a refusal) still means the host is up —
   * something answered; only `filtered` (a timeout/unreachable) is no evidence.
   *
   * Windows reports a refusal only after ~2 s of SYN retries, so a `timeoutMs`
   * under that reads a refused port as `filtered` there; `open` is instant
   * everywhere.
   */
  type Liveness = "open" | "closed" | "filtered"

  /** One address on a {@link NetInterface}. */
  type InterfaceAddr = {
    /** The IP address. */
    ip: string
    /** CIDR prefix length (e.g. 24). */
    prefix: number
    /** Address family. */
    family: "v4" | "v6"
  }

  /** A local network interface, from {@link interfaces}. */
  type NetInterface = {
    /** Interface name, e.g. "wlan0". */
    name: string
    /** Hardware (MAC) address, or `null` if none. */
    mac: string | null
    /** Whether the interface is up. */
    up: boolean
    /** Whether it holds the default route (the interface other hosts reach). */
    default: boolean
    /** Whether it is a loopback interface. */
    loopback: boolean
    /** Whether it supports multicast. */
    multicast: boolean
    /** The interface's bound addresses. */
    addrs: InterfaceAddr[]
  }

  /** A received datagram, from {@link Udp.recv}. */
  type Datagram = {
    /** The payload bytes. */
    data: Uint8Array
    /** Sender IP. */
    host: string
    /** Sender port. */
    port: number
  }

  /**
   * A connected TCP stream: a byte duplex as a `readable`/`writable` pair of
   * web streams, the shape sockets have in every runtime with web streams.
   * `for await (let chunk of conn.readable)` reads it, `conn.readable
   * .pipeTo(other.writable)` is one half of a proxy, and a pipe carries the
   * half-close: when the source ends, the destination's write side closes.
   * A TLS connection ({@link connectTls}, {@link startTls},
   * {@link listenTls}) is the same class: the streams carry the plain text.
   */
  export class Conn {
    /** The remote peer's address, e.g. "192.168.2.37:445". */
    readonly remoteAddr: string
    /** The protocol agreed by ALPN, or `null` (plain TCP, or none agreed). */
    readonly alpnProtocol: string | null
    /**
     * The SHA-256 of the peer's SubjectPublicKeyInfo as lowercase hex, or
     * `null` (plain TCP, or the peer sent no certificate). The same form as
     * {@link TlsClientOptions.pin}.
     */
    readonly peerKey: string | null
    /**
     * Whether the TLS handshake resumed an earlier session (see
     * {@link connectTls}); on a {@link listenTls} connection, whether the
     * server resumed one. `false` on plain TCP and after a full handshake.
     */
    readonly resumed: boolean
    /**
     * What the peer sends, one chunk per read, pulled only as far as it is
     * read. Ends when the peer ends its write side or closes, or on
     * {@link close}; errors with the reason after `writable.abort(reason)`.
     * Over TLS, a peer that closes without `close_notify` also ends it.
     * `cancel()` stops reading: the write side stays usable and the
     * connection stays open until {@link close}, and {@link startTls} can
     * still upgrade it.
     */
    readonly readable: ReadableStream<Uint8Array>
    /**
     * What to send: strings (as UTF-8) or `Uint8Array`s. A write resolves
     * once the OS took the bytes, so a producer faster than the peer reads
     * sees backpressure. `close()` is the half-close: it flushes and ends the
     * write side, so the peer sees end-of-stream while this side keeps
     * reading until the peer closes (the way to say "request done" to a
     * protocol that answers after EOF); a later write rejects. `abort(reason)`
     * tears the connection down with an error: a pending or later read of
     * {@link readable} rejects with the reason. TCP has no abortive
     * half-close, so the peer still sees a plain end-of-stream.
     *
     * Over TLS, `close()` sends `close_notify` and then ends the TCP write
     * side. Reading on after it works on TLS 1.3 only: a TLS 1.2 peer
     * usually answers `close_notify` by closing the whole connection.
     */
    readonly writable: WritableStream<string | Uint8Array>
    /**
     * Close the connection now: a pending read ends, a pending write rejects,
     * the peer sees end-of-stream at once. Bytes already handed to the OS
     * still flush. Over TLS, `close_notify` goes out first if the socket
     * takes it without waiting.
     */
    close(): void
  }

  /**
   * A bound TCP listener: an async-iterable of incoming connections, so
   * `for await (let conn of listener)` accepts them. `close()` to stop.
   */
  export class Listener implements AsyncIterable<Conn> {
    /** The bound local address (with the OS-assigned port when 0 was requested). Empty once closed. */
    readonly localAddr: string
    /** Close the listener: release the port and end the iteration. */
    close(): void
    /**
     * Renew the certificate of a {@link listenTls} listener while it
     * listens: handshakes from now on present the new `cert` and `key`
     * (PEM, as in {@link ListenTlsOptions}), and the connections accepted
     * before keep the certificate they were made with. A
     * {@link generateCertificate} result passes as it is. The client policy
     * and the ALPN list stay as the listener started (`caCerts`,
     * `anyClientCert` and `alpn` throw). Throws, keeping the current
     * certificate, when either is not PEM or the key is not the
     * certificate's. Throws on a {@link listen} listener.
     */
    setCertificate(tls: { cert: string; key: string }): void
    [Symbol.asyncIterator](): AsyncIterator<Conn>
  }

  /** A bound UDP socket with the broadcast/multicast controls a peer beacon needs. */
  export class Udp {
    /** The bound local address (with the OS-assigned port when 0 was requested). Empty once closed. */
    readonly localAddr: string
    /** Send a datagram to `host:port` — a unicast peer, a broadcast address, or a multicast group. */
    send(data: string | Uint8Array, host: string, port: number): Promise<void>
    /** Receive the next datagram, or `null` once the socket is closed. */
    recv(): Promise<Datagram | null>
    /** Close the socket: release it (and its bound port) and resolve a pending {@link recv} with `null`. */
    close(): void
    /** Allow sending to the broadcast address (SO_BROADCAST). */
    setBroadcast(on: boolean): void
    /** TTL for outgoing multicast (1 keeps it on the local link). */
    setMulticastTtl(ttl: number): void
    /** Whether multicast this socket sends loops back to sockets on this host. */
    setMulticastLoop(on: boolean): void
    /**
     * Join multicast `group` on the interface with address `iface`
     * (default "0.0.0.0", OS-chosen). Required to receive that group's datagrams.
     */
    joinMulticast(group: string, iface?: string): void
    /** Leave a multicast group previously joined with {@link joinMulticast}. */
    leaveMulticast(group: string, iface?: string): void
  }

  /**
   * Outcome of an {@link icmpEcho}. Infallible like {@link probe}: `timeout`
   * covers every "no evidence" case (no reply, unreachable, resolve failure);
   * `unsupported` means this platform/configuration has no unprivileged ICMP
   * socket, so "host silent" and "cannot ask" stay distinguishable.
   */
  type IcmpEchoResult =
    | { status: "reply"; rttMs: number; payload: Uint8Array }
    | { status: "timeout" }
    | { status: "unsupported" }

  /**
   * Probe `host:port` with a TCP connect and report what it says about the host.
   * Infallible — every outcome maps to a {@link Liveness}, so a sweep never has to
   * catch. The connect-scan primitive: count `open` or `closed` as a live host.
   *
   * @param opts  timeoutMs (default 1000).
   */
  export function probe(host: string, port: number, opts?: ConnectOptions): Promise<Liveness>

  /**
   * Open a TCP connection. Unlike {@link probe} this returns a live {@link Conn}
   * for app protocols / banner grabs.
   *
   * @param opts  timeoutMs (default 10000).
   */
  export function connect(host: string, port: number, opts?: ConnectOptions): Promise<Conn>

  /** Bind a TCP {@link Listener} on `port` (0 = OS-assigned). */
  export function listen(port: number, opts?: ListenOptions): Promise<Listener>

  /**
   * Open a TLS connection: the TCP connect, then the handshake as the
   * client. `host` is the name the server is verified as and the SNI. A
   * failed handshake or a pin mismatch rejects.
   *
   * On TLS 1.3 the client finishes its handshake before the server checks
   * a client certificate. So a server that rejects the certificate (or a
   * missing one) is seen as an error on the first read, not here.
   *
   * Sessions resume on their own, as in browsers and Node
   * ({@link TlsClientOptions.resume} opts out or names the session). A
   * connection whose options (`caCerts`, `cert` and `key`, `pin`, `alpn`)
   * equal those of an earlier connection of this app, isolates included, to
   * the same server name offers to resume that session; the server decides,
   * and {@link Conn.resumed} reports the outcome. This is what an FTPS data
   * connection (`PROT P`) needs: it carries the control connection's
   * options, so it offers the control connection's session. A resumed
   * handshake verifies no certificate, so other options never share a
   * session.
   *
   * On TLS 1.3 a session is a set of tickets: the server sends them after
   * the handshake, the client stores them only when it reads from that
   * connection, and each resumption spends one. So read from the first
   * connection before you open the second (FTP reads the control reply
   * anyway), and expect a full handshake once the tickets are spent by
   * connections that were never read.
   *
   * Only with the "tls" capability; without it this throws.
   */
  export function connectTls(host: string, port: number, opts?: TlsClientOptions): Promise<Conn>

  /**
   * Upgrade an open plain {@link Conn} to TLS as the client (SMTP and IMAP
   * `STARTTLS`, the PostgreSQL `SSLRequest`), and resolve the new
   * {@link Conn}. The old one is spent at once: its `readable` ends (a
   * pending read as end-of-stream) and its writes reject. It works whatever
   * was done with the old `readable` (a released reader, a `break` out of
   * `for await`, a `cancel()`, a pending read). No plain-text bytes are lost:
   * the stream holds no chunk it was not asked for. Rejects for a connection
   * that is TLS already, closed or half-closed, or whose peer ended.
   *
   * With `isServer: true` it upgrades as the server instead (Node's
   * `isServer`), for an SMTP, IMAP or XMPP server that answers `STARTTLS`:
   * the options are those of {@link listenTls} without `host`. The same
   * guarantees hold: the old `Conn` is spent at once and no plain-text bytes
   * are lost. So send the plain-text answer ("go ahead") and call this at
   * once, with no read of your own pending on the old `readable`: a read
   * that is still waiting when the client's handshake arrives takes its
   * first bytes.
   * {@link Conn.peerKey} reports the client's key when a client certificate
   * was asked for.
   *
   * Either way the handshake must finish within `timeoutMs` (default
   * 10000), or this rejects.
   *
   * Sessions resume as with {@link connectTls}, and
   * {@link TlsClientOptions.resume} works the same: an earlier session of
   * either function is offered. On TLS 1.3, read from the first connection
   * before you upgrade the second. As the server, each call has a session
   * cache of its own, so no client resumes into it.
   *
   * Only with the "tls" capability; without it this throws.
   */
  export function startTls(conn: Conn, opts: StartTlsOptions): Promise<Conn>

  /**
   * Bind a TLS {@link Listener} on `port` (0 = OS-assigned). Its connections
   * arrive with the handshake done: a client that stalls does not hold up
   * the next one, and a failed handshake (or one longer than 10 s) is
   * dropped and never arrives. Renew its certificate with
   * {@link Listener.setCertificate}.
   *
   * Only with the "tls" capability; without it this throws.
   */
  export function listenTls(port: number, opts: ListenTlsOptions): Promise<Listener>

  /**
   * Make a TLS identity: a fresh ECDSA P-256 key and a certificate for it,
   * valid for `validDays`, as PEM (the key in PKCS#8), with the
   * key's pin in the {@link Conn.peerKey} form. The result goes straight
   * into {@link listenTls}, {@link startTls}, `serve({ tls })` of
   * `flux:http`, or the `cert` and `key` of a client.
   *
   * What it is for: a LAN app that pairs by pin makes its identity on
   * first run, self-signed, and its peers store the pin they saw at
   * pairing ({@link TlsClientOptions.pin}). A fleet makes a private CA once
   * (`isCa: true`) and issues each device a certificate with it
   * (`issuer`); clients trust the CA through `caCerts`.
   *
   * The runtime stores nothing: keep the PEM yourself (with `flux:fs`) and
   * load it on the next run, or the identity changes. Synchronous: a P-256
   * key takes well under a millisecond. The verb is Web Crypto's
   * (`generateKey`). Throws for options that do not fit (no names, an
   * `issuer` that does not parse).
   *
   * Only with the "tls" capability; without it this throws.
   */
  export function generateCertificate(opts: GenerateCertificateOptions): GeneratedCertificate

  /** Bind a {@link Udp} socket. */
  export function udp(opts?: UdpOptions): Promise<Udp>

  /**
   * Ping `host` once: send an ICMP echo request carrying `payload` (default
   * empty) and wait for the matching reply. IPv4 only.
   *
   * @param opts  timeoutMs (default 1000).
   */
  export function icmpEcho(host: string, payload?: string | Uint8Array, opts?: ConnectOptions): Promise<IcmpEchoResult>

  /**
   * Enumerate local network interfaces and their addresses — the no-subprocess
   * way to find the subnet to scan (replaces parsing `ip addr`). Synchronous.
   */
  export function interfaces(): NetInterface[]
}
