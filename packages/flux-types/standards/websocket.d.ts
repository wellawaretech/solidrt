// The web-standard WebSocket client. A deliberate subset: handler properties
// only (no addEventListener), plain-object events (not Event instances), ws://
// only (wss:// is not supported yet), and send accepts only string or Uint8Array.

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

interface WebSocket {
  readonly url: string
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

declare let WebSocket: {
  /** Open a connection. `url` must be ws:// (wss:// is not supported yet). */
  new (url: string): WebSocket
  readonly CONNECTING: 0
  readonly OPEN: 1
  readonly CLOSING: 2
  readonly CLOSED: 3
}