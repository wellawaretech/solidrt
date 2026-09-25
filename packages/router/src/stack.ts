// The navigation state and its transitions, headless: a parent stack, and
// with a tabs route in the tree a stack per tab under it. Plain functions
// over plain data, no signals; router.tsx keeps the state in a signal and
// applies these. Tested under bun without a client (tests/).
//
// Without tabs the state is the one stack of paths it always was. With
// tabs, the parent stack holds the full-window screens and one marker
// entry, the tabs route's own path, that stands for the tab bar screen;
// which tab shows and what each tab has pushed is in `tabs`. This is the
// navigator tree every native router keeps (a root stack, tabs, a stack per
// tab), written as data.
import { formatPath, formatPattern, matchPath, tabsRoutes } from "./route"
import type { AnyRoute, Match } from "./route"

export interface TabsState {
  /** The tab that shows: its root path. */
  active: string
  /** Each tab's stack, bottom first; the bottom is always the tab's root. */
  stacks: Record<string, string[]>
}

export interface NavState {
  /** The parent stack, bottom first; `tabs.path` in it is the tab bar screen. */
  stack: string[]
  tabs: TabsState | null
}

/**
 * The stack as plain data: an array of paths without tabs, the parent stack
 * plus the tabs with them. What `entries()` returns and `initial` takes.
 */
export type Entries = string[] | { stack: string[]; tabs: TabsState }

/** The tree's tabs route, resolved once. */
export interface TabsInfo {
  route: AnyRoute
  /** The route's own path: the parent-stack entry that stands for the tab bar. */
  path: string
  /** The tabs' root paths, in declaration order; the first is home. */
  roots: string[]
}

export function findTabs(root: AnyRoute): TabsInfo | null {
  let found = tabsRoutes(root)
  if (found.length === 0) return null
  if (found.length > 1) {
    throw new Error(`Router: one tabs route per tree ("${formatPattern(found[0]!)}" and "${formatPattern(found[1]!)}")`)
  }
  let route = found[0]!
  return { route, path: formatPath(route), roots: route.children.map((tab) => formatPath(tab)) }
}

/** A path resolved against the tree: where it lands. */
export interface Target {
  path: string
  matches: Match[]
  /** The tab it falls under (its root path), or null for the parent stack. */
  tab: string | null
}

/**
 * Where `path` lands, or null when nothing matches. A path that names the
 * tabs route itself, with no tab under it, lands on the first tab's root.
 */
export function resolve(tree: AnyRoute, tabs: TabsInfo | null, path: string): Target | null {
  let matches = matchPath(tree, path)
  if (!matches) return null
  if (!tabs) return { path, matches, tab: null }
  let at = matches.findIndex((m) => m.route === tabs.route)
  if (at < 0) return { path, matches, tab: null }
  let tab = matches[at + 1]
  if (!tab) return resolve(tree, tabs, tabs.roots[0]!)
  return { path, matches, tab: formatPath(tab.route) }
}

// Every tab on its root, `active` showing.
function allRoots(tabs: TabsInfo, active: string): TabsState {
  let stacks: Record<string, string[]> = {}
  for (let root of tabs.roots) stacks[root] = [root]
  return { active, stacks }
}

// The parent stack with the tab bar on top: cut back to the marker when it
// is there (the screens above it leave), appended when it is not. The same
// array when it already is on top.
function showTabs(stack: string[], marker: string): string[] {
  let at = stack.lastIndexOf(marker)
  if (at === stack.length - 1) return stack
  return at >= 0 ? stack.slice(0, at + 1) : [...stack, marker]
}

// `state` with tab `tab` showing and its stack replaced.
function withTab(state: NavState, tabs: TabsInfo, tab: string, stack: string[]): NavState {
  let current = state.tabs ?? allRoots(tabs, tab)
  let same = current.stacks[tab] === stack
  return {
    stack: showTabs(state.stack, tabs.path),
    tabs: { active: tab, stacks: same ? current.stacks : { ...current.stacks, [tab]: stack } },
  }
}

// A tab's root as a target is the tab bar's tap: a tab that does not show
// is shown as it was left, the one that shows goes back to its root. The
// stack for the target's tab, or null when the target is a deeper screen.
function toRoot(state: NavState, tabs: TabsInfo, target: Target): string[] | null {
  if (target.path !== target.tab) return null
  let current = state.tabs?.stacks[target.tab!] ?? [target.tab!]
  let showing = state.tabs?.active === target.tab && state.stack[state.stack.length - 1] === tabs.path
  return showing && current.length > 1 ? [target.tab!] : current
}

/** `target` pushed: onto the parent stack, or onto its tab, which then shows. */
export function push(state: NavState, tabs: TabsInfo | null, target: Target): NavState {
  if (!tabs || target.tab === null) return { ...state, stack: [...state.stack, target.path] }
  let current = state.tabs?.stacks[target.tab] ?? [target.tab]
  return withTab(state, tabs, target.tab, toRoot(state, tabs, target) ?? [...current, target.path])
}

/**
 * `target` swapped for the top of the stack that holds it: the parent
 * stack's top for a full-window screen (the tab bar itself when that is
 * showing), the tab's top for a screen in a tab, which then shows. A tab's
 * root is never swapped out, and a tab's root as the target is a tap on
 * its tab, as for push.
 */
export function replace(state: NavState, tabs: TabsInfo | null, target: Target): NavState {
  if (!tabs || target.tab === null) return { ...state, stack: [...state.stack.slice(0, -1), target.path] }
  let current = state.tabs?.stacks[target.tab] ?? [target.tab]
  let next =
    toRoot(state, tabs, target) ?? (current.length > 1 ? [...current.slice(0, -1), target.path] : [target.tab, target.path])
  return withTab(state, tabs, target.tab, next)
}

/**
 * The state started over at `target`: every tab on its root, and the
 * target's tab root beneath it when it is in a tab. A full-window target
 * stands alone, so back leaves the app (a flow that ends outside the tabs).
 */
export function reset(tabs: TabsInfo | null, target: Target): NavState {
  if (!tabs) return { stack: [target.path], tabs: null }
  if (target.tab === null) return { stack: [target.path], tabs: allRoots(tabs, tabs.roots[0]!) }
  let all = allRoots(tabs, target.tab)
  all.stacks[target.tab] = target.path === target.tab ? [target.tab] : [target.tab, target.path]
  return { stack: [tabs.path], tabs: all }
}

/**
 * One step back: the tab bar showing pops its active tab, or switches a
 * non-first tab at its root to the first; otherwise the parent stack pops.
 * Null when there is nothing to pop.
 */
export function back(state: NavState, tabs: TabsInfo | null): NavState | null {
  if (tabs && state.tabs && state.stack[state.stack.length - 1] === tabs.path) {
    let active = state.tabs.active
    let current = state.tabs.stacks[active] ?? [active]
    if (current.length > 1) return withTab(state, tabs, active, current.slice(0, -1))
    if (active !== tabs.roots[0]) return { ...state, tabs: { ...state.tabs, active: tabs.roots[0]! } }
  }
  if (state.stack.length > 1) return { ...state, stack: state.stack.slice(0, -1) }
  return null
}

/** The path that shows: the parent stack's top, or the active tab's. */
export function currentPath(state: NavState, tabs: TabsInfo | null): string | undefined {
  let top = state.stack[state.stack.length - 1]
  if (tabs && state.tabs && top === tabs.path) {
    let stack = state.tabs.stacks[state.tabs.active]
    return stack?.[stack.length - 1]
  }
  return top
}

/**
 * The scopes whose stacks `next` changes from `prev`: null for the parent
 * stack, a tab's root for its stack. What a navigation leaves, so whose
 * blockers it asks; a bare tab switch changes no stack and asks none.
 */
export function changed(prev: NavState, next: NavState): Array<string | null> {
  let out: Array<string | null> = []
  if (prev.stack !== next.stack) out.push(null)
  if (prev.tabs && next.tabs && prev.tabs.stacks !== next.tabs.stacks) {
    for (let tab of Object.keys(next.tabs.stacks)) {
      if (prev.tabs.stacks[tab] !== next.tabs.stacks[tab]) out.push(tab)
    }
  }
  return out
}

/**
 * The state `initial` names, or the launch link's. A path lands where a
 * push from nothing would, a tab's root beneath a screen in a tab; a list
 * of paths is pushed in order; an `entries()` object is taken as saved
 * when every entry still fits the tree, and not at all otherwise (what
 * the app should show instead is the app's call, not a repair). Paths
 * that match nothing are dropped with a warning, and with nothing left
 * the app starts at "/".
 */
export function initialState(
  tree: AnyRoute,
  tabs: TabsInfo | null,
  initial: string | Entries | undefined,
  normalize: (path: string) => string,
): NavState {
  let target = (path: string): Target | null => {
    let t = resolve(tree, tabs, normalize(path))
    if (!t) console.warn(`Router: no route matches "${path}"; dropped from the initial stack`)
    return t
  }
  if (initial !== undefined && !Array.isArray(initial) && typeof initial !== "string") {
    if (fits(tree, tabs, initial, normalize)) return { stack: initial.stack, tabs: tabs ? initial.tabs : null }
    initial = undefined
  }
  let paths = initial === undefined ? ["/"] : typeof initial === "string" ? [initial] : initial
  let state: NavState | null = null
  for (let path of paths) {
    let t = target(path)
    if (!t) continue
    state = state ? push(state, tabs, t) : reset(tabs, t)
  }
  return state ?? { stack: ["/"], tabs: tabs ? allRoots(tabs, tabs.roots[0]!) : null }
}

// Whether a saved `entries()` object still describes this tree: a parent
// entry is the tabs marker or lands outside the tabs, every tab is there
// with its root at the bottom and its entries landing in it, and `active`
// is a tab. The first misfit is named in a warning.
function fits(
  tree: AnyRoute,
  tabs: TabsInfo | null,
  saved: { stack: string[]; tabs: TabsState },
  normalize: (path: string) => string,
): boolean {
  let misfit = (what: string): false => {
    console.warn(`Router: the saved stack does not fit the route tree (${what}); not resumed`)
    return false
  }
  let lands = (path: string, tab: string | null) => resolve(tree, tabs, normalize(path))?.tab === tab
  for (let path of saved.stack) {
    if (!(tabs && path === tabs.path) && !lands(path, null)) return misfit(`"${path}"`)
  }
  if (!tabs) return saved.stack.length > 0 || misfit("empty")
  if (!tabs.roots.includes(saved.tabs.active)) return misfit(`active tab "${saved.tabs.active}"`)
  for (let root of tabs.roots) {
    let entries = saved.tabs.stacks[root]
    if (!entries || entries[0] !== root) return misfit(`tab "${root}"`)
    for (let path of entries) if (!lands(path, root)) return misfit(`"${path}" in tab "${root}"`)
  }
  return saved.stack.length > 0 || misfit("empty")
}
