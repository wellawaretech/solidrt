// The Fetch API cluster (Headers, Request, Response, fetch). A deliberate subset
// of the WHATWG Fetch standard: flux provides exactly these members and no more
// (no Blob, FormData, ReadableStream, clone(), AbortSignal, ...).
// Grouped in one file because the four share BodyInit/HeadersInit and reference
// each other.

/** Header initializer: a plain name -> value object, or another Headers. */
type HeadersInit = Record<string, string> | Headers

/**
 * A message body: a string, raw bytes, or an async-iterable of string/byte
 * chunks (e.g. an `async function*`), which is sent as a stream.
 */
type BodyInit = string | Uint8Array | AsyncIterable<string | Uint8Array>

/** A subset of the WHATWG Headers API. Names are case-insensitive. */
interface Headers {
  /** The value for `name` (multiple values comma-joined), or null if absent. */
  get(name: string): string | null
  /** Set `name` to `value`, replacing any existing values. */
  set(name: string, value: string): void
  /** Whether any value for `name` exists. */
  has(name: string): boolean
  /** Remove `name`. */
  delete(name: string): void
  /** Add a value for `name` without replacing existing ones. */
  append(name: string, value: string): void
  /**
   * Call `callback(value, name, headers)` for each entry. Iterates entries as
   * stored (insertion order, duplicates separate); WHATWG iterates sorted with
   * duplicate names combined.
   */
  forEach(callback: (value: string, name: string, headers: Headers) => void, thisArg?: any): void
}

declare let Headers: {
  new (init?: HeadersInit): Headers
}

interface RequestInit {
  /** HTTP method; uppercased. Defaults to "GET". */
  method?: string
  /** Request body. */
  body?: BodyInit | null
  /** Request headers. */
  headers?: HeadersInit
  /**
   * Disk-cache policy, explicit and per call. flux never caches by default
   * (like Node/Bun/Deno), and server cache headers (`cache-control`,
   * `expires`, `etag`) are ignored entirely: the caller decides.
   *
   * - `"force-cache"`: serve from disk if stored, otherwise fetch and store.
   *   No freshness, no TTL: the entry lives until evicted by the size cap or
   *   overwritten by `"reload"`. Use for assets (images, audio, fonts);
   *   versioned URLs are the normal way to handle updatable assets.
   * - `"reload"`: fetch fresh and overwrite the stored entry.
   * - `"default"`, `"no-store"`, `"no-cache"`: accepted, plain network
   *   request (all equivalent to omitting the option here; there is no
   *   freshness model to modulate).
   *
   * Only GET requests with 2xx responses are cached, keyed by URL; on other
   * methods the option is ignored. Unknown values throw.
   */
  cache?: "force-cache" | "reload" | "default" | "no-store" | "no-cache"
}

/**
 * A subset of the WHATWG Request. flux adds `params` (route params) for requests
 * the `flux:http` server passes to handlers. The body is read-once.
 */
interface Request {
  readonly method: string
  readonly url: string
  readonly headers: Headers
  /** Route params from the matched pattern; an empty object for a JS-constructed Request. */
  readonly params: Record<string, string>
  /** The body as an async-iterable of byte chunks (read once). */
  readonly body: AsyncIterable<Uint8Array>
  /** Read the whole body as UTF-8 text. */
  text(): Promise<string>
  /** Read the whole body as raw bytes. */
  bytes(): Promise<Uint8Array>
  /** Read the whole body as an ArrayBuffer. */
  arrayBuffer(): Promise<ArrayBuffer>
  /** Read and parse the whole body as JSON. */
  json(): Promise<any>
}

declare let Request: {
  new (url: string, init?: RequestInit): Request
}

interface ResponseInit {
  /** Status code. Defaults to 200. */
  status?: number
  /** Status text. */
  statusText?: string
  /** Response headers. */
  headers?: HeadersInit
}

/** A subset of the WHATWG Response. The body is read-once. */
interface Response {
  readonly status: number
  readonly statusText: string
  /** True when `status` is in the range 200..299. */
  readonly ok: boolean
  readonly url: string
  readonly headers: Headers
  /** The body as an async-iterable of byte chunks (read once). */
  readonly body: AsyncIterable<Uint8Array>
  /** Read the whole body as UTF-8 text. */
  text(): Promise<string>
  /** Read the whole body as raw bytes. */
  bytes(): Promise<Uint8Array>
  /** Read the whole body as an ArrayBuffer. */
  arrayBuffer(): Promise<ArrayBuffer>
  /** Read and parse the whole body as JSON. */
  json(): Promise<any>
}

declare let Response: {
  new (body?: BodyInit | null, init?: ResponseInit): Response
  /** Build a JSON response (sets Content-Type to application/json when unset). */
  json(data: any, init?: ResponseInit): Response
}

/**
 * Fetch a resource over HTTP(S). The body may be a string, Uint8Array, or an
 * async-iterable (streamed). Resolves to a {@link Response}.
 */
declare function fetch(url: string, options?: RequestInit): Promise<Response>