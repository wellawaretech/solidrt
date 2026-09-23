// The Solid binding over route.ts: the stack, navigation, the back step,
// blocking, links in and the location out, the JSX tree (`<Route>`) and the
// components that render the matched routes. Everything here is built on
// what core offers every app (onBack, onLink, env.launchLink,
// reportLocation from srt:dev); core knows nothing of routes.
import { createSignal, createMemo, createContext, createEffect, useContext, untrack, onCleanup, children, Show } from "@solidrt/core"
import { env, onBack, onLink } from "@solidrt/core"
import { reportLocation } from "srt:dev"
import { createRootRoute, createRoute, formatPath, linkToPath, matchPath, place } from "./route"
import type { AnyRoute, Match, ParamsOf, RawParams, Route as RouteValue } from "./route"

/** The route value (see route.ts); `Route` the component is the JSX form. */
export type Route<P = {}> = RouteValue<P>

/** Where the app is: the top of the stack, matched. */
export interface Location {
  /** The path, root-first with params filled in; the stack's own currency. */
  path: string
  /** The matched routes root-first; the last is the screen. */
  matches: Match[]
}

/**
 * A navigation target: a path (as a link or a tool would give it), a route
 * with no params, or a route with the params its pattern needs. The route
 * forms are checked at compile time; the path form is matched at run time.
 */
export type NavTarget<R extends AnyRoute = AnyRoute> = string | Route<{}> | { route: R; params: ParamsOf<R> }

export interface NavigateOptions {
  /** Swap the current entry instead of pushing over it. */
  replace?: boolean
  /**
   * Start the stack over at this entry: the app's home after a flow is
   * finished (a scan dialed, a login done), where back should leave the app
   * rather than revisit the flow.
   */
  reset?: boolean
}

/**
 * A guard on leaving the current entry: false keeps the app where it is;
 * true, or a promise resolving to true, lets the navigation through. The
 * promise form holds it while the user answers a dialog.
 */
export type Blocker = () => boolean | Promise<boolean>

export interface RouterOptions {
  /** The root of the route tree (createRootRoute). */
  tree: AnyRoute
  /**
   * Where the app starts: a path, or a saved stack (`entries()`) to resume
   * with. A launch link (env.launchLink) wins over it: the user just asked
   * for that screen. Defaults to "/".
   */
  initial?: string | string[]
}

export interface Router {
  /** The stack as plain data, bottom first; save it in onSuspend. Reactive. */
  entries(): string[]
  /** The matched top of the stack, or null when nothing matches. Reactive. */
  location(): Location | null
  /**
   * Push a target onto the stack (or replace the top, or reset to it).
   * Returns false, and warns, when no route matches; a blocker may cancel it
   * later, which the returned promise reports.
   */
  navigate(target: NavTarget, options?: NavigateOptions): Promise<boolean>
  /** Pop the stack. Resolves false at the root (nothing to pop) or when blocked. */
  back(): Promise<boolean>
  /** The path a target names; what a link to it would carry. */
  href(target: NavTarget): string
  /** @internal the blockers registered by useBlocker */
  blockers: Set<Blocker>
  /** @internal the tree */
  tree: AnyRoute
}

/**
 * The router's state, created once per app (at module scope or in the app
 * component) and rendered by `<Router router={...}>`. Navigation is a
 * signal write, so Solid's transition semantics apply: a screen whose async
 * reads are pending keeps the previous one on screen, `isPending(() =>
 * router.location())` says so.
 */
export function createRouter(options: RouterOptions): Router {
  let tree = options.tree
  let launch = env.launchLink
  let start = launch != null ? [linkToPath(launch)] : normalizeInitial(options.initial)
  let valid = start.filter((path) => {
    if (matchPath(tree, path)) return true
    console.warn(`Router: no route matches "${path}"; dropped from the initial stack`)
    return false
  })
  let [entries, setEntries] = createSignal<string[]>(valid.length > 0 ? valid : ["/"])
  let location = createMemo<Location | null>(() => {
    let list = entries()
    let path = list[list.length - 1]
    if (path === undefined) return null
    let matches = matchPath(tree, path)
    return matches ? { path, matches } : null
  })
  let blockers = new Set<Blocker>()

  let href = (target: NavTarget): string => {
    if (typeof target === "string") return linkToPath(target)
    if ("route" in target) return formatPath(target.route, target.params as Record<string, unknown>)
    return formatPath(target)
  }

  // Runs the blockers; a sync false cancels at once, promises are awaited
  // together and any false cancels.
  let allowed = async (): Promise<boolean> => {
    let pending: Promise<boolean>[] = []
    for (let block of blockers) {
      let verdict = block()
      if (verdict === false) return false
      if (verdict !== true) pending.push(verdict)
    }
    if (pending.length === 0) return true
    return (await Promise.all(pending)).every(Boolean)
  }

  let router: Router = {
    entries,
    location,
    href,
    blockers,
    tree,
    async navigate(target, options = {}) {
      let path = href(target)
      if (!matchPath(tree, path)) {
        console.warn(`Router: no route matches "${path}"`)
        return false
      }
      if (!(await allowed())) return false
      setEntries((list) => {
        if (options.reset) return [path]
        if (options.replace) return [...list.slice(0, -1), path]
        return [...list, path]
      })
      return true
    },
    async back() {
      if (untrack(entries).length <= 1) return false
      if (!(await allowed())) return false
      setEntries((list) => list.slice(0, -1))
      return true
    },
  }
  return router
}

function normalizeInitial(initial: string | string[] | undefined): string[] {
  if (initial === undefined) return ["/"]
  if (typeof initial === "string") return [linkToPath(initial)]
  return initial.map(linkToPath)
}

const RouterContext = createContext<Router>()
// How deep in the matched chain the surrounding route is; an Outlet renders
// the next one.
const DepthContext = createContext<number>(0)

export type RouterProps =
  /** A router made with createRouter, over a tree of route values. */
  | { router: Router; initial?: never; children?: never }
  /**
   * A tree of `<Route>` elements, in matching order, under a root with no
   * component; the router is made here and reached through the hooks.
   */
  | { router?: never; initial?: string | string[]; children: any }

/**
 * Renders the matched routes and owns the app's step of the back stack, the
 * links in and the location out. Mount it once, inside the window. Its
 * `onBack` registers first, so every screen and dialog above it gets the
 * event before the stack pops.
 */
export function Router(props: RouterProps) {
  let router = untrack(
    () =>
      props.router ??
      createRouter({ tree: createRootRoute({ children: routesOf(props.children) }), initial: props.initial }),
  )

  onBack((e) => {
    // Nothing to pop at the root: the platform's default (background on
    // Android, exit elsewhere) runs unless something above prevented it.
    if (untrack(router.entries).length <= 1) return
    e.preventDefault()
    void router.back()
  })
  onLink((link) => {
    void router.navigate(linkToPath(link))
  })
  // The location out: tooling reads it, and a hot reload re-enters it.
  createEffect(
    () => router.location()?.path ?? null,
    (path) => reportLocation(path),
  )
  onCleanup(() => reportLocation(null))

  return (
    <RouterContext value={router}>
      <RouteView depth={0} />
    </RouterContext>
  )
}

export interface RouteProps {
  /** This route's own path below its parent (see createRoute); `/` without one. */
  path?: string
  /** What renders here; without one, an outlet alone. */
  component?: () => any
  /**
   * A route value to place here instead: typed or validated params live on
   * the value (createRoute), the JSX gives it its place in the tree. Not
   * with `path` or `component`.
   */
  route?: AnyRoute
  /** Nested `<Route>` elements, in matching order. */
  children?: any
}

/**
 * One route of a JSX tree, under `<Router>` or another `<Route>`. Not an
 * element: it evaluates to a route value that its parent places.
 */
export function Route(props: RouteProps): any {
  return untrack(() => {
    let below = routesOf(props.children)
    if (props.route) {
      if (props.path !== undefined || props.component !== undefined) {
        throw new Error("<Route route={...}> takes no path or component: they are the route's own")
      }
      place(props.route, below)
      return props.route
    }
    return createRoute({ path: props.path ?? "/", component: props.component, children: below })
  })
}

// The route values a `children` slot holds, resolved once; anything else
// there is a mistake (a `<Route>` is not an element and cannot mix with
// them).
function routesOf(slot: any): AnyRoute[] {
  let list = children(() => slot).toArray()
  for (let item of list) {
    if (typeof item !== "object" || item === null || !("segments" in item)) {
      throw new Error("<Router> and <Route> take only <Route> elements as children")
    }
  }
  return list as unknown as AnyRoute[]
}

// The route at `depth` of the current match chain. Keyed on the route, not
// the match: params changing under the same route update reactively through
// useParams, without a remount.
function RouteView(props: { depth: number }) {
  let router = useContext(RouterContext)
  let route = createMemo(() => router.location()?.matches[props.depth]?.route ?? null)
  return (
    <DepthContext value={props.depth}>
      <Show when={route()} keyed>
        {(r: AnyRoute) => (r.component ? <r.component /> : <Outlet />)}
      </Show>
    </DepthContext>
  )
}

/** Where a layout route's matched child renders. */
export function Outlet() {
  let depth = useContext(DepthContext)
  return <RouteView depth={depth + 1} />
}

/** The router this component renders under. */
export function useRouter(): Router {
  return useContext(RouterContext)
}

/** The current location, reactive; null when nothing matches. */
export function useLocation(): () => Location | null {
  return useContext(RouterContext).location
}

/**
 * The params of the current location. With a route (the one this component
 * renders at, or an ancestor) they are typed by it; without one they are
 * the screen's raw strings, as a JSX `<Route path>` carries them. Reactive:
 * a navigation to the same route with other params updates it in place.
 */
export function useParams(): () => RawParams
export function useParams<R extends AnyRoute>(route: R): () => ParamsOf<R>
export function useParams(route?: AnyRoute): () => any {
  let router = useContext(RouterContext)
  return createMemo(() => {
    let matches = router.location()?.matches ?? []
    let match = (route && matches.find((m) => m.route === route)) ?? matches[matches.length - 1]
    return match?.params ?? {}
  })
}

/** `navigate` bound to the router this component renders under. */
export function useNavigate(): (target: NavTarget, options?: NavigateOptions) => Promise<boolean> {
  let router = useContext(RouterContext)
  return (target, options) => router.navigate(target, options)
}

/**
 * A link: its path (for display or a tool) and a `go()` for a press handler.
 * Styling is the caller's; a components Link wraps it.
 */
export function createLink(target: NavTarget, options?: NavigateOptions): { href: string; go: () => Promise<boolean> } {
  let router = useContext(RouterContext)
  return { href: router.href(target), go: () => router.navigate(target, options) }
}

/**
 * Guards leaving the current entry while this component is mounted: back,
 * navigate and replace all ask `block` first. Return false to stay, true to
 * go, or a promise (a confirm dialog) that decides later. A blocked back is
 * prevented, so it never falls through to the platform default.
 */
export function useBlocker(block: Blocker): void {
  let router = useContext(RouterContext)
  router.blockers.add(block)
  onCleanup(() => router.blockers.delete(block))
}
