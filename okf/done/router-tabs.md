---
title: Router tabs
description: "A `tabs` route in @solidrt/router: each child is a tab with its own stack, kept mounted and hidden while inactive, under a parent stack that holds the full-window screens above the tab bar; re-tap goes to the tab's root, back on a non-first tab's root goes to the first tab, links land in their tab. The model every native router shares (React Navigation, Expo Router, go_router, UIKit/SwiftUI, Jetpack); web routers have nothing here."
created: 2026-09-25
completed: 2026-09-25
---

# Router tabs

The router ([app-routing](../plans/app-routing.md)) has one stack for the
whole app, and every entry is a full path. Layouts nest to any depth but
keep no history of their own, so a tab bar built on it today is sibling
routes switched with `navigate(tab, { reset: true })`: each tab forgets
where it was, and back from any tab leaves the app. Tabbed apps expect
one stack per tab, and that is the next thing the first tabbed app asks
for.

## What other routers do

Tabs are a native-router concern; web routers (TanStack, Solid Router,
Vue Router) have one history and at most `keep-alive`, which is why the
routing plan's survey never met this. The native routers agree:

- **React Navigation**: navigators nest as a tree, each owns its state,
  `Root Stack > Tabs > Stack per tab`. Tapping the active tab pops its
  stack to the top; `backBehavior: "firstRoute"` by default (back on a
  non-first tab's root goes to the first tab, on the first tab's root it
  leaves the app); tabs mount lazily and stay mounted, `freezeOnBlur`
  pauses an inactive one; a full-screen or modal screen lives in the root
  stack above the tabs (hiding the tab bar for a screen inside a tab is a
  documented awkwardness); a deep link into a nested screen builds the
  nested state with the tab's initial route beneath it.
- **Expo Router**: the same model file-based, a `(tabs)` group under a
  root `<Stack>` that also holds modals; tab screens have plain paths.
- **go_router** (Flutter): `StatefulShellRoute.indexedStack(branches)`,
  the closest analog to a route-config design. Each branch keeps its own
  navigator in an `IndexedStack`; the shell builder receives a
  `navigationShell` handle; `goBranch(i, initialLocation: i == current)`
  is the re-tap-to-root. Back on a branch root exits.
- **UIKit / SwiftUI**: `UITabBarController` pops the selected tab to its
  root on re-tap; a `NavigationStack` with its own path per tab; full
  screen covers presented at the `TabView` level.
- **Jetpack Navigation**: a saved back stack per tab
  (`saveState`/`restoreState`), back from a non-start tab returns to the
  start destination. Navigation 3 (2025) makes the back stack app-owned
  plain data and multiple stacks the app's own state to model, which is
  what `entries()` already is here.

## Design

### The tree

A route marked `tabs` makes each of its children a tab. A tab is named
by its root path, the router's usual unit, so the same name works in a
JSX tree, a value tree and a `NavShell` item.

```tsx
<Router>
  <Route path="/" tabs component={Main}>          // each child is a tab
    <Route path="/feed">
      <Route path="/" component={Feed} />
      <Route path="/post/$id" component={Post} />
    </Route>
    <Route path="/search" component={Search} />
    <Route path="/profile" component={Profile} />
  </Route>
  <Route path="/viewer/$id" component={Viewer} />  // full window, above the tabs
  <Route path="/$" component={NotFound} />
</Router>

function Main() {
  let tabs = useTabs()   // { paths: string[], active(): string, select(path) }
  return (
    <NavShell items={items} value={tabs.active()} onChange={(v) => tabs.select(v as string)}>
      <Outlet />
    </NavShell>
  )
}
```

`createRoute({ tabs: true, children })` is the value form. Rules checked
when the tree is built: a tab's own route takes no required params (the
tab bar has none to give it); a `tabs` route inside a tab is refused
(React Navigation permits and discourages it; refuse until an app asks).
The router stays ignorant of components: `NavShell` is wired to
`useTabs()` in app code, and an app can draw any bar.

### The state

A tree without a `tabs` route is one stack, exactly as today. With one,
the state mirrors the navigator tree the native routers use:

```
{ stack: [...paths], tabs: { active: "/feed", stacks: { "/feed": [...], "/search": [...] } } }
```

`stack` is the parent stack: its bottom entry is the tabs route itself
(the tab bar screen), everything above it is a full-window screen over
the tabs. Each tab's stack holds only screens under the tab bar, so back
from a full-window screen returns to the tabs as one unit, and the saved
state reads like the tree. `entries()` returns this shape, `initial`
accepts it (a plain path or path array still works and lands in the tab
it names); `onSuspend` restore is unchanged in principle.

`location()` and the reported path stay one path: the top of the parent
stack, or the active tab's top when the parent stack is just the tabs
entry. Tooling (`get_location`, `open_link`, `srt render --link`) and hot
reload re-entry need no change.

### Behaviour

The consensus above, taken as-is:

- Tapping another tab shows it with its stack as left. Tapping the active
  tab pops it to its root. Navigating to a tab's root path is that tap:
  `useTabs().select(path)` is `navigate(path)`.
- Navigating to a screen inside a tab switches to that tab and pushes it
  on the tab's stack; the screens above the tab bar, if any, leave first
  (a tab screen cannot show under them).
- Navigating to a screen outside the tabs pushes it on the parent stack:
  full window, no tab bar. Back returns to the tabs, active tab intact.
- Back pops the parent stack while a screen is above the tab bar; then
  the active tab's stack while it is deeper than its root; then, on a
  non-first tab's root, switches to the first tab; then pops the parent
  stack if the tab bar has something beneath it (a login it was pushed
  over); on nothing it leaves the event to the platform, as today.
- `replace` swaps the top of the stack that holds the target: the parent
  stack's top for a full-window target, which is the tab bar itself when
  that shows (the tabs leave the parent stack; a later tab navigation
  pushes the bar back); a tab's top for a tab target, never its root.
  `reset` clears every stack and lands the target with its tab's root
  beneath it; a full-window target stands alone (a flow that ends outside
  the tabs, a logout).
- A link arriving while the app runs is a navigation. A launch link or
  `initial` path into a tab lands with the tab's root beneath it (the
  routing plan's deferred "synthesized parent stack", pulled in here
  because a tab has an obvious root), the other tabs on their roots; a
  full-window path stands alone, as for any non-tab app. A list of paths
  is pushed in order; an `entries()` object is taken back as saved when
  every entry still fits the tree, and not at all otherwise (one warning
  naming the misfit): a save from another route tree is the app's to
  handle, not the router's to repair into a state no run ever had.
- A move asks the blockers of the scopes whose stacks it changes: the
  parent stack's (components outside the tabs, the tab bar's component
  included) and each tab's whose stack it pops, pushes or swaps. A bare
  switch, by tap or by back to the first tab, changes no stack and asks
  none; a reset asks every tab.

### Rendering

A tab mounts the first time it becomes active and stays mounted; an
inactive tab is `display="none"`. The render tree's hidden pass lays a
hidden subtree out to a zero box, and paint, hit and envelope walks never
enter it (`rendertree/mod.rs` `is_hidden`), so a hidden tab costs
nothing to draw; the layout-box queries return nothing for a node that is
not laid out (`tree/geometry.rs`), so `createFocusNav` cannot reach a
hidden tab's controls. Verified in the source and on the running client
2026-09-25 (a hidden tab's text nodes report a zero box; its `shown false`
text still updated while hidden). Reactive code in a hidden tab keeps
running: `useTabs().active()` compared with the tab's own path is the
`freezeOnBlur` equivalent, for a screen to pause video or per-frame work.

The tabs route and each tab render in a full-size `<view>` (flex 1,
column) of their own: a host node is needed to hide, and the tabs route
must stay mounted under a full-window screen (a single keyed chain would
unmount it, and every tab with it). The tabs route's host lives at its
depth of the chain, so it survives as long as the routes above it stay
matched: true for a full-window screen that is a sibling of the tabs
route, the shape recommended.

Per-tab context: `useParams`, `useLocation` and `useBlocker` inside a
tab read that tab's stack, not the app-wide top (a hidden tab reading the
active tab's params would return `{}` where the type says `{ id: number
}`). Every native router scopes these to the navigator; here each tab's
host provides a scope (the tab's location memo, the blocker scope) in
context, and the hooks read through it. `useNavigate` is global: the
target decides the stack.

## Implementation

Built 2026-09-25.

1. `route.ts`: the `tabs` flag on `RouteOptions` and the JSX `<Route>`;
   the rules checked at `place` over the whole subtree placed (a value
   tree is built bottom-up, so the ancestors only appear with the
   placement above); `tabsRoutes(root)`.
2. `stack.ts`, headless: `NavState` (`{ stack, tabs }`, the `entries()`
   shape), `findTabs`, `resolve` (where a path lands; the tabs route's
   own path lands on the first tab), `push`/`replace`/`reset`/`back`,
   `changed` (the scopes a move touches, for the blockers), `initialState`
   (path, list, saved object, launch link). No tabs route: one stack, as
   before.
3. `router.tsx`: the state in one signal, the app location and a location
   memo per tab, blockers per scope, the tabs host in `RouteView`, the
   per-tab hosts in `Outlet`, `useTabs()`.
4. `tests/stack.test.ts` for the headless parts; the Solid binding checked
   on the dev server with `probes/router-tabs-probe.tsx` (NavShell wired to
   `useTabs()`): tab state kept across switches, re-tap to root, back at
   every level, a full-window screen over the tabs and back, a link into a
   tab and one to the catch-all, reset and replace, the blocker scopes,
   hot reload landing at the last location.
5. README and AGENTS notes for the router; the components AGENTS points
   at `useTabs()` for wiring `NavShell`.

## Findings

- The first cut treated a tab's root path as "pop to root" whatever tab
  showed, and the tab bar's `select` is `navigate(root)`: tapping another
  tab then popped it to its root instead of showing it as left. Found on
  the client, not in the tests, which had encoded the wrong rule. The
  rule is the native one: a tab's root as target is a tap on its tab.
- With the tabs kept mounted under a full-window screen, the app-level
  chain still switches from the tabs route to the screen at the tabs
  route's depth; the tabs host is a second slot at that depth, keyed on
  "seen", next to the keyed `Show` that renders anything else there.

## Decisions taken

- **Full-window screens on the parent stack**, not the current tab's
  stack. Every native router models it this way; a tab's stack then only
  holds screens under the tab bar, and the saved state mirrors the tree.
  The price is the nested state shape, which is what React Navigation's
  state looks like anyway.
- **Back on a non-first tab's root goes to the first tab.** The Android
  and React Navigation default; iOS has no back at a tab root, and desktop
  back is exit either way. A `backBehavior` option waits for an app that
  wants `none`.
- **Visited tabs stay mounted.** What every native router does; remounting
  on every switch is simpler but loses scroll position and typed input
  until the routing plan's deferred per-entry retention exists.
- **Tabs are named by path**, not by index. Indices are what go_router
  uses and what makes a reordered tab bar a silent bug.

## Not in this item

- Nested tabs. Refused at tree build; lift when an app asks.
- A tab bar in the router. `NavShell` in components draws one, wired in
  app code; the router only exposes `useTabs()`.
- Per-entry retention (scroll, focus) across pushes inside a tab; that
  is the routing plan's deferred item and applies to every stack alike.
