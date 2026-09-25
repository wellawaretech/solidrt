// The headless half of the router: the route tree, matching a path against
// it, building a path from a route and its params, and turning a link into a
// path. Plain functions over plain data, no rendering, no signals; the Solid
// binding (router.tsx) is the only consumer. Tested under bun without a
// client (tests/).

/** The raw string params a path carries, before a route's `parse`. */
export type RawParams = Record<string, string>

/**
 * A route in the tree. `P` is what this route's own `parse` produces: what
 * `useParams(route)` is typed as (an ancestor's params ride along at run
 * time, read them through the ancestor). A route is a plain value; it gets
 * its parent when it is placed, in a `children` list or under a JSX
 * `<Route>`. Created by createRootRoute / createRoute, never by hand.
 */
export interface Route<P = {}> {
  readonly path: string
  /** Set once, by placement. */
  readonly parent: AnyRoute | null
  readonly children: AnyRoute[]
  readonly component: (() => any) | null
  readonly parse: ((raw: RawParams) => unknown) | null
  readonly segments: Segment[]
  /** Each child is a tab with a stack of its own (see stack.ts). */
  readonly tabs: boolean
  // A phantom carrying the params type. Contravariant on purpose: a route
  // with required params is not a `Route<{}>`, so navigate(route) without
  // params is a type error for it while a param-less route passes bare.
  readonly _params: (params: P) => void
}

export type AnyRoute = Route<any>

/** The params `useParams(R)` returns: what R's own `parse` produces. */
export type ParamsOf<R> = R extends Route<infer P> ? P : never

export interface RouteOptions<P> {
  /**
   * This route's own path, below its parent's: literal segments, `$name` for
   * a param, `$name?` for an optional one (only at the end), `$` for the
   * rest of the path (a catch-all, captured as `rest`). `/` adds nothing and
   * makes an index route (or, with children, a pathless layout).
   */
  path: string
  /**
   * The untrusted-input boundary: turns this route's raw string params into
   * typed values, or throws, in which case the route does not match (the
   * next sibling is tried, then a catch-all). Without it the raw strings are
   * the params (typed RawParams).
   */
  params?: { parse: (raw: RawParams) => P }
  /**
   * What renders at this route; a layout route renders its child through
   * `<Outlet>`. Without one the route is an outlet alone.
   */
  component?: () => any
  /**
   * The routes below this one, in matching order (put a catch-all `$`
   * last). More can be placed later under a JSX `<Route route={...}>`.
   */
  children?: AnyRoute[]
  /**
   * Makes each child a tab: a screen with a stack of its own that stays
   * mounted while another tab shows. A tab's path is literal (the tab bar
   * has no params to give it), the route and its ancestors take no params,
   * and a tree holds one tabs route, not inside another. The component
   * renders the tabs through `<Outlet>` and reaches them with `useTabs()`.
   */
  tabs?: boolean
}

// One piece of a route's path pattern.
export type Segment =
  | { kind: "literal"; value: string }
  | { kind: "param"; name: string; optional: boolean }
  | { kind: "rest" }

export const REST_PARAM = "rest"

function parseSegments(path: string): Segment[] {
  let out: Segment[] = []
  for (let piece of path.split("/")) {
    if (piece === "") continue
    if (piece === "$") {
      out.push({ kind: "rest" })
    } else if (piece.startsWith("$")) {
      let optional = piece.endsWith("?")
      let name = piece.slice(1, optional ? -1 : undefined)
      if (name === "") throw new Error(`Route path "${path}": a param needs a name`)
      out.push({ kind: "param", name, optional })
    } else {
      out.push({ kind: "literal", value: piece })
    }
  }
  // A rest or optional segment ends the pattern: nothing can follow what
  // may consume everything or nothing.
  for (let i = 0; i < out.length - 1; i++) {
    let s = out[i]!
    if (s.kind === "rest" || (s.kind === "param" && s.optional)) {
      throw new Error(`Route path "${path}": "${s.kind === "rest" ? "$" : "$" + s.name + "?"}" must be the last segment`)
    }
  }
  return out
}

/** The tree's root: matches the empty path and holds every other route. */
export function createRootRoute(options: { component?: () => any; children?: AnyRoute[] } = {}): Route<{}> {
  let root: Route<{}> = {
    path: "/",
    parent: null,
    children: [],
    component: options.component ?? null,
    parse: null,
    segments: [],
    tabs: false,
    _params: () => {},
  }
  place(root, options.children ?? [])
  return root
}

/** A route: a value with no parent until it is placed. */
export function createRoute<P = RawParams>(options: RouteOptions<P>): Route<P> {
  let route: Route<P> = {
    path: options.path,
    parent: null,
    children: [],
    component: options.component ?? null,
    parse: options.params?.parse ?? null,
    segments: parseSegments(options.path),
    tabs: options.tabs ?? false,
    _params: () => {},
  }
  place(route, options.children ?? [])
  return route
}

/**
 * Puts `children` under `parent`, after any it already has. A route has one
 * place in one tree: placing it again throws. The tabs rules are checked
 * here too, over the whole subtree placed, since a value tree is built
 * bottom-up and an ancestor only appears with the placement above.
 */
export function place(parent: AnyRoute, children: AnyRoute[]): void {
  for (let child of children) {
    if (child.parent) {
      throw new Error(`Route "${formatPattern(child)}" is already placed; a route has one place in the tree`)
    }
    ;(child as { parent: AnyRoute | null }).parent = parent
    parent.children.push(child)
    checkTabs(child)
  }
}

// The tabs rules for `route` and everything below it (see RouteOptions.tabs).
function checkTabs(route: AnyRoute): void {
  if (route.parent?.tabs) {
    if (route.segments.some((s) => s.kind !== "literal")) {
      throw new Error(`Tab "${formatPattern(route)}": a tab's path is literal`)
    }
    let own = formatPattern(route)
    for (let sibling of route.parent.children) {
      if (sibling !== route && formatPattern(sibling) === own) {
        throw new Error(`Tab "${own}" is declared twice`)
      }
    }
  }
  if (route.tabs) {
    for (let anc = route.parent; anc; anc = anc.parent) {
      if (anc.tabs) throw new Error(`Route "${formatPattern(route)}": tabs inside a tab are not supported`)
      if (anc.segments.some((s) => s.kind !== "literal")) {
        throw new Error(`Route "${formatPattern(route)}": a tabs route takes no params above it`)
      }
    }
  }
  for (let child of route.children) checkTabs(child)
}

/** The tabs routes in the tree; a router takes one. */
export function tabsRoutes(root: AnyRoute): AnyRoute[] {
  let out: AnyRoute[] = []
  let walk = (route: AnyRoute) => {
    if (route.tabs) out.push(route)
    for (let child of route.children) walk(child)
  }
  walk(root)
  return out
}

/** One route of a matched path, with the params it parsed. */
export interface Match {
  route: AnyRoute
  params: Record<string, unknown>
}

function splitPath(path: string): string[] {
  return path
    .split("/")
    .filter((s) => s !== "")
    .map((s) => {
      try {
        return decodeURIComponent(s)
      } catch {
        return s
      }
    })
}

// Consumes this route's own pattern from the front of `segments`; the raw
// params it captured and what is left, or null when the pattern does not fit.
function consume(route: AnyRoute, segments: string[]): { raw: RawParams; rest: string[] } | null {
  let raw: RawParams = {}
  let i = 0
  for (let s of route.segments) {
    if (s.kind === "rest") {
      raw[REST_PARAM] = segments.slice(i).join("/")
      return { raw, rest: [] }
    }
    let piece = segments[i]
    if (piece === undefined) {
      if (s.kind === "param" && s.optional) return { raw, rest: [] }
      return null
    }
    if (s.kind === "literal") {
      if (piece !== s.value) return null
    } else {
      raw[s.name] = piece
    }
    i++
  }
  return { raw, rest: segments.slice(i) }
}

// Matches `route` and, through its children, the whole of `segments`; the
// chain root-first, or null. A route whose `parse` throws does not match.
function matchFrom(route: AnyRoute, segments: string[], inherited: Record<string, unknown>): Match[] | null {
  let own = consume(route, segments)
  if (!own) return null
  let params: Record<string, unknown>
  try {
    params = { ...inherited, ...(route.parse ? (route.parse(own.raw) as Record<string, unknown>) : own.raw) }
  } catch {
    return null
  }
  let here: Match = { route, params }
  if (own.rest.length === 0) {
    // An index child (`/`) takes an exact match over the layout alone.
    for (let child of route.children) {
      if (child.segments.length === 0) {
        let below = matchFrom(child, [], params)
        if (below) return [here, ...below]
      }
    }
    return [here]
  }
  for (let child of route.children) {
    let below = matchFrom(child, own.rest, params)
    if (below) return [here, ...below]
  }
  return null
}

/**
 * The routes `path` names, root-first, each with its params, or null when
 * the tree has no route for it (and no catch-all).
 */
export function matchPath(root: AnyRoute, path: string): Match[] | null {
  return matchFrom(root, splitPath(path), {})
}

/**
 * The path of `route` with `params` filled in, root-first; the reverse of
 * matchPath. An optional param that is missing is left out; the rest param
 * is appended as given.
 */
export function formatPath(route: AnyRoute, params: Record<string, unknown> = {}): string {
  let chain: AnyRoute[] = []
  for (let r: AnyRoute | null = route; r; r = r.parent) chain.unshift(r)
  let out: string[] = []
  for (let r of chain) {
    for (let s of r.segments) {
      if (s.kind === "literal") {
        out.push(s.value)
      } else if (s.kind === "rest") {
        let rest = params[REST_PARAM]
        if (rest !== undefined && rest !== null && rest !== "") out.push(String(rest))
      } else {
        let value = params[s.name]
        if (value === undefined || value === null) {
          if (s.optional) continue
          throw new Error(`Route "${formatPattern(route)}": missing param "${s.name}"`)
        }
        out.push(encodeURIComponent(String(value)))
      }
    }
  }
  return "/" + out.join("/")
}

/** The full pattern of `route`, root-first, for messages. */
export function formatPattern(route: AnyRoute): string {
  let parts: string[] = []
  for (let r: AnyRoute | null = route; r; r = r.parent) {
    let own = r.path.replace(/^\/+|\/+$/g, "")
    if (own) parts.unshift(own)
  }
  return "/" + parts.join("/")
}

/**
 * The path a link names. A custom-scheme link (`myapp://settings/theme`,
 * `com.example.app:/item/42`) is the whole of what follows the scheme, as
 * apps conventionally read them; an http(s) link drops its host. A query or
 * fragment is dropped: locations are paths. A bare path passes through with
 * a leading slash.
 */
export function linkToPath(link: string): string {
  let m = /^([A-Za-z][A-Za-z0-9+.-]+):(.*)$/s.exec(link)
  let rest = link
  if (m) {
    let scheme = m[1]!.toLowerCase()
    rest = m[2]!
    if (scheme === "http" || scheme === "https") {
      rest = rest.replace(/^\/\/[^/]*/, "")
    } else {
      rest = rest.replace(/^\/\//, "")
    }
  }
  rest = rest.replace(/[?#].*$/s, "")
  return rest.startsWith("/") ? rest : "/" + rest
}
