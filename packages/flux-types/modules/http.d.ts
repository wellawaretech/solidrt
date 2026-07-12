declare module "flux:http" {
  import type { Endpoint } from "flux:p2p"

  /** Path parameters captured from a route pattern (e.g. ":page"). */
  type RouteParams = Record<string, string>

  /**
   * The request passed to a handler: a standard {@link Request} plus the route
   * params captured from the matched pattern.
   */
  type FluxRequest = Request & {
    /**
     * Route params from the matched pattern (e.g. `:page` -> `params.page`). An
     * empty object for the `fetch` fallback, which matches no pattern.
     */
    params: RouteParams
  }

  /**
   * What a handler may return: a string (sent as a 200 text response), a
   * {@link Response}, or a promise of either. Returning nothing is only valid
   * after `server.upgrade(req)` accepted a websocket; otherwise it becomes a 500.
   */
  type HandlerResult = string | Response | void | Promise<string | Response | void>

  /**
   * Handles a matched route or the `fetch` fallback. Receives the request (with
   * captured `params`) and the running {@link Server}.
   */
  type RouteHandler = (req: FluxRequest, server: Server) => HandlerResult

  /**
   * A per-method route object, e.g. `{ GET, POST }`. A request whose method has
   * no entry gets a 405 with an `Allow` header listing the defined methods.
   */
  type MethodRoutes = {
    GET?: RouteHandler
    HEAD?: RouteHandler
    POST?: RouteHandler
    PUT?: RouteHandler
    DELETE?: RouteHandler
    PATCH?: RouteHandler
    OPTIONS?: RouteHandler
  }

  /**
   * A value in the route table: a handler function, a static {@link Response}
   * (snapshotted once at registration and served on every request), or a
   * per-method object.
   */
  type Route = RouteHandler | Response | MethodRoutes

  /**
   * The per-connection socket handle passed to the `websocket` callbacks.
   * Returned send/publish counts are the bytes (or sockets) queued.
   */
  type ServerWebSocket = {
    /**
     * Arbitrary value attached via `upgrade(req, { data })`; `undefined` when
     * none was given. Settable.
     */
    data: any
    /**
     * Queue a message: a string sends a text frame, a Uint8Array a binary frame.
     * Returns the bytes queued, 0 if the socket is no longer open, or -1 when the
     * queue exceeds `backpressureLimit` (the message is still queued and `drain`
     * fires once the queue empties).
     */
    send(data: string | Uint8Array): number
    /**
     * Send a ping control frame; the peer's reply surfaces in the `pong`
     * callback. Payload must be 125 bytes or fewer. Same return values as `send`.
     */
    ping(data?: string | Uint8Array): number
    /** Send an unsolicited pong control frame (125 bytes or fewer). */
    pong(data?: string | Uint8Array): number
    /**
     * Join a topic; `server.publish(topic)` and peers' `ws.publish(topic)` then
     * reach this socket. No-op on a closing or closed socket.
     */
    subscribe(topic: string): void
    /** Leave a topic. Closing the socket unsubscribes everything automatically. */
    unsubscribe(topic: string): void
    /** Whether this socket is currently subscribed to `topic`. */
    isSubscribed(topic: string): boolean
    /**
     * Publish to every subscriber of `topic` except this socket. Returns the
     * number of sockets the message was queued to.
     */
    publish(topic: string, data: string | Uint8Array): number
    /**
     * Send a close frame (default code 1000). The connection finishes once the
     * peer echoes the close, or the grace period expires.
     */
    close(code?: number, reason?: string): void
    /** Connection state: CONNECTING 0, OPEN 1, CLOSING 2, CLOSED 3. */
    readonly readyState: number
    /**
     * The peer's IP address (or, for a connection accepted over the `p2p`
     * option, the peer's endpoint id), or undefined when unknown.
     */
    readonly remoteAddress: string | undefined
  }

  /**
   * The `websocket` serve option: per-server socket lifecycle callbacks, shared
   * by every connection. Incoming pings are answered automatically by the
   * protocol layer and never surface (so there is no `ping` callback).
   */
  type WebSocketHandlers = {
    /** Fired once a connection is established (after `server.upgrade`). */
    open?(ws: ServerWebSocket): void
    /** Fired for each text (string) or binary (Uint8Array) message. */
    message?(ws: ServerWebSocket, data: string | Uint8Array): void
    /** Fired when a backpressured send queue empties. */
    drain?(ws: ServerWebSocket): void
    /** Fired when the peer replies to a `ws.ping()`. */
    pong?(ws: ServerWebSocket, data: Uint8Array): void
    /** Fired once when the connection closes, with the close code and reason. */
    close?(ws: ServerWebSocket, code: number, reason: string): void
    /**
     * Queue-size threshold (bytes) at which `send` returns -1 and `drain` later
     * fires. Defaults to the runtime's built-in limit.
     */
    backpressureLimit?: number
  }

  /** Options for {@link Server.upgrade}. */
  type UpgradeOptions = {
    /** Becomes `ws.data` on the upgraded socket. */
    data?: any
    /**
     * Extra headers appended to the 101 response (e.g. `Set-Cookie`). An invalid
     * header fails the upgrade.
     */
    headers?: Record<string, string> | Headers
  }

  /**
   * A peer address, as returned by {@link Server.requestIP}. A p2p peer has no
   * IP: `address` is its endpoint id, `port` is 0, and `family` is `"p2p"`.
   */
  type SocketAddress = {
    /** The peer's IP address, or a p2p peer's endpoint id. */
    address: string
    /** The peer's port (0 for a p2p peer). */
    port: number
    family: "IPv4" | "IPv6" | "p2p"
  }

  type Server = {
    /** The bound port. */
    readonly port: number
    /** The bound hostname/interface. */
    readonly hostname: string
    /** The server's base URL, e.g. `"http://0.0.0.0:3000/"`. */
    readonly url: string
    /**
     * Accept a websocket handshake for `req`. On `true` the handler must return
     * nothing: the held 101 response is sent when it returns and the `websocket`
     * callbacks take over. `false` means the request cannot upgrade (not a
     * websocket request, already upgraded, or no `websocket` option), so the
     * handler can serve a normal response instead.
     */
    upgrade(req: FluxRequest, opts?: UpgradeOptions): boolean
    /**
     * Publish a message to every socket subscribed to `topic`. Returns the number
     * of sockets the message was queued to.
     */
    publish(topic: string, data: string | Uint8Array): number
    /** How many sockets are currently subscribed to `topic`. */
    subscriberCount(topic: string): number
    /**
     * The peer address of the connection `req` arrived on, or null when unknown
     * (e.g. a JS-constructed Request).
     */
    requestIP(req: Request): SocketAddress | null
    /**
     * Stop accepting new connections and gracefully shut down open ones. Safe to
     * call more than once.
     */
    stop(): void
  }

  /** Options for accepting `flux:p2p` connections alongside the TCP listener. */
  type P2pOptions = {
    /** The `flux:p2p` Endpoint to accept connections on. */
    endpoint: Endpoint
    /** ALPN protocol matched against each incoming connection. */
    protocol: string
  }

  type ServeOptions = {
    /** Port to listen on. */
    port: number
    /** Hostname/interface to bind. Defaults to "0.0.0.0" (all interfaces). */
    hostname?: string
    /**
     * Route table keyed by path pattern. Patterns may contain `:name` segments,
     * exposed on `req.params`. Each value is a handler function, a static
     * {@link Response}, or a per-method object.
     */
    routes?: Record<string, Route>
    /**
     * Fallback handler for requests no route matched. Without it (and with no
     * matching route), unmatched requests get a 404.
     */
    fetch?: RouteHandler
    /**
     * Handles a throw or rejection from a handler; its result becomes the
     * response. Without it, a handler error becomes a plaintext 500.
     */
    error?: (error: any) => string | Response | Promise<string | Response>
    /**
     * WebSocket lifecycle callbacks. Providing this enables `server.upgrade()`;
     * without it `upgrade()` always returns false.
     */
    websocket?: WebSocketHandlers
    /**
     * Accept connections on a `flux:p2p` Endpoint alongside the TCP listener:
     * each incoming connection whose ALPN matches `protocol` has the HTTP/WS
     * protocol spoken over its first bidirectional stream. `server.stop()`
     * stops accepting; the endpoint itself stays open for its owner.
     */
    p2p?: P2pOptions
  }

  /**
   * Start an HTTP server. Loosely models Bun's `Bun.serve`.
   *
   * @param options  Port, hostname, routes, fetch fallback, error handler, and
   *                 websocket callbacks.
   * @returns The running {@link Server}.
   */
  export function serve(options: ServeOptions): Server
}