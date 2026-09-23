# @solidrt/router - agent notes

Routing for SolidRT apps: a typed route tree, a stack, one back step, links
in and the location out. Optional: an app with one screen, or one that
routes by hand over core's `onBack`/`onLink`, needs none of it. Prose is in
README.md; the typed API is src/route.ts (the headless tree and matching)
and src/router.tsx (the Solid binding).

## Install

```sh
bun add @solidrt/router   # peers: @solidrt/core, @solidjs/signals
```

## Shape

```tsx
let item = createRoute({                                    // a value: typed params
  path: "/item/$id",
  params: { parse: (raw) => ({ id: Number(raw.id) }) },   // throw = no match
  component: Item,
})

<Router initial="/">                                        // once, inside <Window>
  <Route path="/" component={Shell}>                        // Shell renders <Outlet />
    <Route path="/" component={Home} />
    <Route path="/settings" component={Settings} />
    <Route route={item} />                                  // the value, placed
    <Route path="/$" component={NotFound} />
  </Route>
</Router>
```

- JSX for the tree; a route value (`createRoute`) where a param needs a
  type or validation, placed with `<Route route={...}>`. A JSX `<Route>`
  takes `path` and `component` only. Values can also form the whole tree
  (`createRootRoute({ children: [...] })`, `createRoute({ ..., children })`)
  with `createRouter({ tree })` at module scope and `<Router router={...}>`;
  the player does this.
- Targets: `useNavigate()("/item/42")` by path (matched at run time, refused
  with a warning when nothing matches), or by value, checked at compile
  time: `navigate(home)`, `navigate({ route: item, params: { id: 42 } })`.
- `useParams()()` is the screen's raw strings; `useParams(item)()` is typed
  by the value. Both reactive. `useLocation()`, `useNavigate()`,
  `useRouter()`, `createLink(target)`, `useBlocker(fn)`.
- `router.entries()` is the stack as paths; pass a saved one as `initial`.
- Path syntax: literal, `$name`, `$name?` (last), `$` for the rest (a `$`
  route last under the root is the not-found screen). `/` is an index
  route, or with children a pathless layout. Children match in
  declaration order.

## Traps

- A `<Route>` is not an element: it evaluates to a route value. Only
  `<Route>` elements go under `<Router>` or another `<Route>`; anything
  else there throws at mount.
- A route value has one place in one tree: placing it twice (two `<Route
  route={x}>`, or a `children:` list and a `<Route route={x}>`) throws.
- With a JSX tree the router exists only inside `<Router>`: screens reach
  it through `useNavigate()` / `useRouter()`, not a module-scope import.
  With a value tree at module scope, screens that import route values from
  the module that imports them form a cycle ESM handles as long as a
  screen reads a route only inside a handler or a hook call, never at
  module scope.
- Route components take no props. State a screen shares with others is a
  module-scope signal (see the player's parts/app-state.ts), not a prop.
- A layout route's component must render `<Outlet />` or its child never
  appears; a route without a component is an outlet alone.
- `<Router>` registers its `onBack` before anything it renders. A dialog
  that registers its own `onBack` inside a screen still runs first, as
  before; only use `useBlocker` for guards that must hold a navigation
  (unsaved changes), not for closing overlays.
- `navigate(..., { reset: true })` for "flow finished, this is home"; a
  plain push there leaves the flow's screens under home for back to revisit.
- Links are untrusted: put validation in `params.parse`, never in the
  screen. `linkToPath` strips a custom scheme whole (`myapp://a/b` is
  `/a/b`) and an http(s) host.
- Tooling: `open_link` / `POST /__control__/link` opens a screen,
  `get_location` / `GET /__control__/link` reads the path the router
  publishes, `srt render --link <path>` renders that screen.
