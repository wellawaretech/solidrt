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
   * A connected TCP stream: a byte duplex. It is its own async iterator, so
   * `for await (let chunk of conn)` reads it until end-of-stream.
   */
  export class Conn implements AsyncIterable<Uint8Array> {
    /** The remote peer's address, e.g. "192.168.2.37:445". */
    readonly peer: string
    /** Write all of `data`. Resolves once it is handed to the OS. */
    write(data: string | Uint8Array): Promise<void>
    /** Stop reading and close the connection. */
    close(): void
    [Symbol.asyncIterator](): AsyncIterator<Uint8Array>
  }

  /**
   * A bound TCP listener: an async-iterable of incoming connections, so
   * `for await (let conn of listener)` accepts them. Drop it to stop.
   */
  export class Listener implements AsyncIterable<Conn> {
    /** The bound local address (with the OS-assigned port when 0 was requested). */
    readonly localAddr: string
    [Symbol.asyncIterator](): AsyncIterator<Conn>
  }

  /** A bound UDP socket with the broadcast/multicast controls a peer beacon needs. */
  export class Udp {
    /** The bound local address (with the OS-assigned port when 0 was requested). */
    readonly localAddr: string
    /** Send a datagram to `host:port` — a unicast peer, a broadcast address, or a multicast group. */
    send(data: string | Uint8Array, host: string, port: number): Promise<void>
    /** Receive the next datagram. */
    recv(): Promise<Datagram>
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

  /** Bind a {@link Udp} socket. */
  export function udp(opts?: UdpOptions): Promise<Udp>

  /**
   * Enumerate local network interfaces and their addresses — the no-subprocess
   * way to find the subnet to scan (replaces parsing `ip addr`). Synchronous.
   */
  export function interfaces(): NetInterface[]
}
