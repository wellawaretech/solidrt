// The web-standard WebSocket client. A deliberate subset: handler properties
// only (no addEventListener), plain-object events (not Event instances), and
// send accepts only string or Uint8Array.

/** The event passed to {@link WebSocket.onopen}. */
interface WebSocketOpenEvent {
  type: "open"
}

/** The event passed to {@link WebSocket.onmessage}. */
interface WebSocketMessageEvent {
  type: "message"
  /** Text frames arrive as a string, binary frames as a Uint8Array. */
  data: string | Uint8Array
}

/** The event passed to {@link WebSocket.onerror}. */
interface WebSocketErrorEvent {
  type: "error"
  message: string
}

/** The event passed to {@link WebSocket.onclose}. */
interface WebSocketCloseEvent {
  type: "close"
  code: number
  reason: string
  wasClean: boolean
}

/**
 * The options form of the second {@link WebSocket} constructor argument,
 * the shape undici (Node) and Bun take.
 */
type WebSocketOptions = {
  /** The subprotocols to offer, as the standard second argument takes them. */
  protocols?: string | string[]
  /**
   * The client whose TLS a `wss://` connection uses: it verifies the server
   * as that client's fetches do (a private CA, a pin) and presents its
   * client certificate. A client that is closed when the socket is made
   * reports `error`, then an unclean 1006 `close`; closing it later does
   * not affect a connection already made. No effect on `ws://`.
   */
  client?: import("flux:http").HttpClient
}

interface WebSocket {
  readonly url: string
  /**
   * The subprotocol the server picked from the offered ones; `""` when it
   * picked none, and before the socket opens.
   */
  readonly protocol: string
  /** Connection state: CONNECTING 0, OPEN 1, CLOSING 2, CLOSED 3. */
  readonly readyState: number
  onopen: ((event: WebSocketOpenEvent) => void) | null
  onmessage: ((event: WebSocketMessageEvent) => void) | null
  onerror: ((event: WebSocketErrorEvent) => void) | null
  onclose: ((event: WebSocketCloseEvent) => void) | null
  /**
   * Queue a message: a string sends a text frame, a Uint8Array a binary frame.
   * Throws while CONNECTING; dropped once the socket is closing or closed.
   */
  send(data: string | Uint8Array): void
  /**
   * Start the closing handshake. `code` must be 1000 or in 3000..4999; `reason`
   * must be 123 bytes or fewer.
   */
  close(code?: number, reason?: string): void
  readonly CONNECTING: 0
  readonly OPEN: 1
  readonly CLOSING: 2
  readonly CLOSED: 3
}

/**
 * The WebSocket client. Only in a build with the "http" capability (the
 * stock runtimes have it); the global is absent without it.
 */
declare let WebSocket: {
  /**
   * Open a connection. `url` is `ws://` (default port 80) or `wss://` (TLS,
   * default port 443). A `wss://` server is verified as `fetch` verifies
   * one: its certificate must chain to the built-in web roots and name the
   * host. With a `client` in the options, it is verified as that client's
   * fetches verify (a private CA, a pin), and the client's certificate is
   * presented. A failed TLS handshake reports `error`, then an unclean 1006
   * `close`, as a refused connection does.
   *
   * The second argument is the standard one: the subprotocols to offer, a
   * string or an array of strings, sent as `Sec-WebSocket-Protocol`. Each
   * must be a non-empty token, none twice, else this throws a
   * `SyntaxError`. In its place an options object takes the subprotocols
   * and the `client`. The server picks one of them or none
   * ({@link WebSocket.protocol}); a pick that was not offered fails the
   * connection: `error`, then an unclean 1006 `close`, and no `open`.
   */
  new (url: string, protocols?: string | string[] | WebSocketOptions): WebSocket
  readonly CONNECTING: 0
  readonly OPEN: 1
  readonly CLOSING: 2
  readonly CLOSED: 3
}