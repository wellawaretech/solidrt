declare module "flux:http" {
  /** Path parameters captured from a route pattern (e.g. ":page"). */
  type RouteParams = Record<string, string>

  /** The request passed to a route handler, with captured route params. */
  type FluxRequest = Request & {
    params: RouteParams
  }

  /** Handles a matched route, returning a `Response` (or a promise of one). */
  type RouteHandler = (req: FluxRequest) => Response | Promise<Response>

  type ServeOptions = {
    /** Port to listen on. */
    port?: number
    /** Hostname/interface to bind. Defaults to all interfaces. */
    hostname?: string
    /**
     * Route table keyed by path pattern. Patterns may contain `:name`
     * segments, exposed on `req.params`.
     */
    routes: Record<string, RouteHandler>
  }

  type Server = {
    port: number
    hostname: string
    /** Stop accepting connections and shut the server down. */
    stop(): void
  }

  /**
   * Start an HTTP server with the given route table.
   *
   * @param options  Port, hostname, and routes.
   * @returns The running {@link Server}.
   */
  export function serve(options: ServeOptions): Server
}