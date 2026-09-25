// The Solid binding over route.ts and stack.ts: the state in a signal, the
// navigation, the back step, blocking, links in and the location out, the
// JSX tree (`<Route>`) and the components that render the matched routes.
// Everything here is built on what core offers every app (onBack, onLink,
// env.launchLink, reportLocation from srt:dev); core knows nothing of routes.
import { createSignal, createMemo, createContext, createEffect, useContext, untrack, onCleanup, children, Show, For } from "@solidrt/core"
import { env, onBack, onLink } from "@solidrt/core"
import { reportLocation } from "srt:dev"
import { createRootRoute, createRoute, formatPath, linkToPath, matchPath, place } from "./route"
import type { AnyRoute, Match, ParamsOf, RawParams, Route as RouteValue } from "./route"
import { back as stepBack, changed, currentPath, findTabs, initialState, push, replace, reset, resolve } from "./stack"
import type { Entries, NavState, TabsInfo } from "./stack"

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
   * rather than revisit the flow. With tabs, every tab returns to its root.
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
   * Where the app starts: a path, or a saved `entries()` to resume with. A
   * launch link (env.launchLink) wins over it: the user just asked for that
   * screen. Defaults to "/".
   */
  initial?: string | Entries
}

/** The tabs of a tabs route, from `useTabs()`. */
export interface Tabs {
  /** The tabs' root paths, in declaration order; the first is home. */
  paths: string[]
  /** The tab that shows, as its root path. Reactive. */
  active(): string
  /**
   * Show a tab: another tab as it was left, the active one back at its
   * root. Resolves false when a blocker held it.
   */
  select(path: string): Promise<boolean>
}

export interface Router {
  /**
   * The stack as plain data, bottom first: an array of paths, or with
   * tabs the parent stack and the tabs. Save it in onSuspend. Reactive.
   */
  entries(): Entries
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
  /** @internal the state as stack.ts sees it */
  state(): NavState
  /** @internal the tree's tabs route, or null */
  tabs: TabsInfo | null
  /** @internal the tab that shows, or null without tabs */
  activeTab(): string | null
  /** @internal a tab's own location: the top of its stack, matched */
  tabLocation(tab: string): () => Location | null
  /** @internal the blockers registered by useBlocker, per scope (null: the parent stack) */
  blockers: Map<string | null, Set<Blocker>>
  /** @internal the tree */
  tree: AnyRoute
}

/**
 * The router's state, created once per app (at module scope or in the app
 * component) and rendered by `<Router router={...}>`. Navigation is a
 * signal write, so Solid's transition semantics apply: a screen whose async
 * reads are pending keeps the previous one on screen, `isPending(() =>
 * router.location()` says so.
 */
export function createRouter(options: RouterOptions): Router {
  let tree = options.tree
  let tabs = findTabs(tree)
  let launch = env.launchLink
  let [state, setState] = createSignal<NavState>(
    initialState(tree, tabs, launch != null ? launch : options.initial, linkToPath),
  )
  let located = (path: string | undefined): Location | null => {
    if (path === undefined) return null
    let matches = matchPath(tree, path)
    return matches ? { path, matches } : null
  }
  let location = createMemo<Location | null>(() => located(currentPath(state(), tabs)))
  let tabLocations = new Map<string, () => Location | null>()
  for (let root of tabs?.roots ?? []) {
    tabLocations.set(
      root,
      createMemo<Location | null>(() => {
        let stack = state().tabs?.stacks[root]
        return located(stack?.[stack.length - 1])
      }),
    )
  }
  let blockers = new Map<string | null, Set<Blocker>>()

  let href = (target: NavTarget): string => {
    if (typeof target === "string") return linkToPath(target)
    if ("route" in target) return formatPath(target.route, target.params as Record<string, unknown>)
    return formatPath(target)
  }

  // Runs the blockers of the given scopes; a sync false cancels at once,
  // promises are awaited together and any false cancels.
  let allowed = async (scopes: Array<string | null>): Promise<boolean> => {
    let pending: Promise<boolean>[] = []
    for (let scope of scopes) {
      for (let block of blockers.get(scope) ?? []) {
        let verdict = block()
        if (verdict === false) return false
        if (verdict !== true) pending.push(verdict)
      }
    }
    if (pending.length === 0) return true
    return (await Promise.all(pending)).every(Boolean)
  }

  let router: Router = {
    entries: () => {
      let s = state()
      return s.tabs ? { stack: s.stack, tabs: s.tabs } : s.stack
    },
    location,
    href,
    state,
    tabs,
    activeTab: () => state().tabs?.active ?? null,
    tabLocation: (tab) => tabLocations.get(tab) ?? (() => null),
    blockers,
    tree,
    async navigate(target, options = {}) {
      let path = href(target)
      let to = resolve(tree, tabs, path)
      if (!to) {
        console.warn(`Router: no route matches "${path}"`)
        return false
      }
      let apply = (s: NavState) => (options.reset ? reset(tabs, to) : options.replace ? replace(s, tabs, to) : push(s, tabs, to))
      // The blockers asked are those of the stacks the move changes; the
      // state is read again after they answer, in case it moved meanwhile.
      let before = untrack(state)
      if (!(await allowed(changed(before, apply(before))))) return false
      setState((s) => apply(s))
      return true
    },
    async back() {
      let before = untrack(state)
      let next = stepBack(before, tabs)
      if (!next) return false
      if (!(await allowed(changed(before, next)))) return false
      setState((s) => stepBack(s, tabs) ?? s)
      return true
    },
  }
  return router
}

const RouterContext = createContext<Router>()
// How deep in the matched chain the surrounding route is; an Outlet renders
// the next one.
const DepthContext = createContext<number>(0)
// The location a subtree renders from: the app's, or inside a tab that
// tab's own, so a hidden tab keeps its params and screen while another
// shows. `tab` is the blocker scope.
interface Scope {
  location: () => Location | null
  tab: string | null
}
const ScopeContext = createContext<Scope>()

export type RouterProps =
  /** A router made with createRouter, over a tree of route values. */
  | { router: Router; initial?: never; children?: never }
  /**
   * A tree of `<Route>` elements, in matching order, under a root with no
   * component; the router is made here and reached through the hooks.
   */
  | { router?: never; initial?: string | Entries; children: any }

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
    if (stepBack(untrack(router.state), router.tabs) === null) return
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
      <ScopeContext value={{ location: router.location, tab: null }}>
        <RouteView depth={0} />
      </ScopeContext>
    </RouterContext>
  )
}

export interface RouteProps {
  /** This route's own path below its parent (see createRoute); `/` without one. */
  path?: string
  /** What renders here; without one, an outlet alone. */
  component?: () => any
  /** Each child is a tab with a stack of its own (see createRoute). */
  tabs?: boolean
  /**
   * A route value to place here instead: typed or validated params live on
   * the value (createRoute), the JSX gives it its place in the tree. Not
   * with `path`, `component` or `tabs`.
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
      if (props.path !== undefined || props.component !== undefined || props.tabs !== undefined) {
        throw new Error("<Route route={...}> takes no path, component or tabs: they are the route's own")
      }
      place(props.route, below)
      return props.route
    }
    return createRoute({ path: props.path ?? "/", component: props.component, tabs: props.tabs, children: below })
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

// The route at `depth` of the scope's match chain. Keyed on the route, not
// the match: params changing under the same route update reactively through
// useParams, without a remount. The tabs route is the exception: once
// mounted at its depth it stays, hidden while another route shows there (a
// full-window screen over the tab bar), so the tabs keep their state.
function RouteView(props: { depth: number }) {
  let router = useContext(RouterContext)
  let scope = useContext(ScopeContext)
  let route = createMemo(() => scope.location()?.matches[props.depth]?.route ?? null)
  let tabsRoute = router.tabs?.route ?? null
  let tabsSeen = createMemo(() => tabsRoute !== null && route() === tabsRoute)
  let seen = false
  let tabsMounted = createMemo(() => (seen ||= tabsSeen()))
  let render = (r: AnyRoute) => (r.component ? <r.component /> : <Outlet />)
  return (
    <DepthContext value={props.depth}>
      <Show when={tabsMounted()}>
        <view flex={1} flexDirection="column" display={tabsSeen() ? "flex" : "none"}>
          {render(tabsRoute!)}
        </view>
      </Show>
      <Show when={tabsSeen() ? null : route()} keyed>
        {render}
      </Show>
    </DepthContext>
  )
}

/**
 * Where a layout route's matched child renders. Under the tabs route it
 * holds every tab visited so far, each in its own full-size view, the
 * inactive ones hidden.
 */
export function Outlet() {
  let router = useContext(RouterContext)
  let depth = useContext(DepthContext)
  let scope = useContext(ScopeContext)
  let here = untrack(() => scope.location()?.matches[depth]?.route ?? null)
  if (here !== null && here === router.tabs?.route) return <TabsOutlet depth={depth} />
  return <RouteView depth={depth + 1} />
}

// The tabs under the tabs route at `depth`: mounted on first visit, kept
// after, hidden while inactive. A hidden tab is laid out to nothing, so it
// is never painted, hit or focused; its reactive code keeps running.
function TabsOutlet(props: { depth: number }) {
  let router = useContext(RouterContext)
  let seen: string[] = []
  let visited = createMemo(() => {
    let active = router.activeTab()
    if (active !== null && !seen.includes(active)) seen = [...seen, active]
    return seen
  })
  return (
    <For each={visited()}>
      {(tab: string) => (
        <view flex={1} flexDirection="column" display={router.activeTab() === tab ? "flex" : "none"}>
          <ScopeContext value={{ location: router.tabLocation(tab), tab }}>
            <RouteView depth={props.depth + 1} />
          </ScopeContext>
        </view>
      )}
    </For>
  )
}

/** The router this component renders under. */
export function useRouter(): Router {
  return useContext(RouterContext)
}

/**
 * The current location, reactive; null when nothing matches. Inside a tab
 * it is that tab's own, whether or not the tab shows.
 */
export function useLocation(): () => Location | null {
  return useContext(ScopeContext).location
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
  let scope = useContext(ScopeContext)
  return createMemo(() => {
    let matches = scope.location()?.matches ?? []
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
 * The tabs of the tree's tabs route, for the component that draws the tab
 * bar (and for a screen to learn whether its tab shows). Throws without a
 * tabs route.
 */
export function useTabs(): Tabs {
  let router = useContext(RouterContext)
  let tabs = router.tabs
  if (!tabs) throw new Error("useTabs: the route tree has no tabs route")
  return {
    paths: tabs.roots,
    active: () => router.activeTab() ?? tabs.roots[0]!,
    select: (path) => router.navigate(path),
  }
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
 * navigate and replace ask `block` first when they change the stack this
 * component is in (a tab's own, or the parent stack). Return false to stay,
 * true to go, or a promise (a confirm dialog) that decides later. A
 * blocked back is prevented, so it never falls through to the platform
 * default. Showing another tab changes no stack, so it asks no blocker.
 */
export function useBlocker(block: Blocker): void {
  let router = useContext(RouterContext)
  let scope = useContext(ScopeContext).tab
  let set = router.blockers.get(scope)
  if (!set) router.blockers.set(scope, (set = new Set()))
  set.add(block)
  onCleanup(() => set.delete(block))
}
