# Router

`@solidrt/router` makes screens addressable. A route tree maps a path like
`/settings/theme` to a screen, a stack holds where the app is, and the same
path is what a link from the OS, an MCP tool, `srt render --link` or a saved
session carries. It is a layer over core: core reports links (`onLink`,
`env.launchLink`) and the back intent (`onBack`), the router decides what
they mean. An app can route by hand without it.

Not a web router. There is no address bar, no forward, no history API, no
code splitting, no data loading. What is left is small, and typed.

## Routes

The tree is JSX, one `<Route>` per screen, nested for layouts:

```tsx
import { Router, Route, Outlet, useParams, useNavigate } from "@solidrt/router"

render(() => (
  <Window>
    <Router initial="/">
      <Route path="/" component={Shell}>
        <Route path="/" component={Home} />
        <Route path="/settings" component={SettingsLayout}>
          <Route path="/$section" component={Section} />
        </Route>
        <Route path="/$" component={NotFound} />
      </Route>
    </Router>
  </Window>
))

function Section() {
  let params = useParams()              // () => { section: string }
  return <text>{params().section}</text>
}
```

A route's `path` is its own piece below the parent: literal segments,
`$name` for a param, `$name?` for an optional one at the end, `$` for the
rest of the path (captured as `rest`; a `$` route last under the root is
the not-found screen). `/` adds nothing: an index route under a layout, or
with children a pathless layout (the `Shell` above). A layout route's
component renders its child with `<Outlet />`; a route without a component
is an `<Outlet />` alone. Children match in declaration order, first match
wins, so a catch-all goes last.

A `<Route>` is not an element: it evaluates to a route value that its
parent places in the tree, once, when `<Router>` mounts. The router made
this way is reached through the hooks (`useNavigate`, `useRouter`).

## Params are validated

A path arrives as strings, and from outside the app they are untrusted. A
JSX route hands them to its screen as they came (`useParams()` above). When
a param has a shape, a route is a value with a `parse`, placed in the JSX
tree where it belongs:

```tsx
import { createRoute } from "@solidrt/router"

let item = createRoute({
  path: "/item/$id",
  params: { parse: (raw) => ({ id: parseId(raw.id) }) },
  component: Item,
})

<Route route={item} />                  // in the tree, next to the JSX routes

function Item() {
  let params = useParams(item)          // () => { id: number }
  return <text>{params().id}</text>
}
```

`params.parse` is the one place strings become typed values: return them,
or throw, and the route does not match (the next sibling is tried, then
the catch-all). `useParams(route)` is typed by the route it names, so there
is no way to read a value that did not pass. That is the rule: JSX for the
tree, a route value where a param needs a type. A JSX `<Route>` takes no
`params`.

Params are reactive: navigating to the same route with other params updates
`params()` without remounting the screen. An ancestor's params are in the
match too; read them through the ancestor, `useParams(settings)`.

### The tree as values

The whole tree can be values instead, with the router made at module scope
so screens import it directly; the player app does this. Children are a
list, in matching order, and a route has one place in one tree:

```tsx
let root = createRootRoute({
  component: Shell,
  children: [home, settings, item, notFound],
})
let router = createRouter({ tree: root, initial: "/" })

<Router router={router} />
```

`<Route route={x}>` and a `children:` list are the same placement; a JSX
`<Route route={settings}>` can nest more `<Route>` elements under a value
that already has children of its own.

## Navigation

```tsx
let navigate = useNavigate()
navigate(home)                                        // a route without params
navigate({ route: section, params: { section: "theme" } })  // one with params, checked
navigate("/settings/theme")                           // a path, as a link would carry it
navigate(home, { replace: true })                     // swap the top instead of pushing
navigate(home, { reset: true })                       // start the stack over here (a flow finished)
router.back()                                         // pop
```

The route forms are checked at compile time: a removed screen or a missing
param is a build error. The path form is matched at run time and refused,
with a warning, when nothing matches.

Every navigation is a signal write, so Solid's transition semantics apply:
a new screen whose async reads are still pending keeps the previous one on
screen, and `isPending(() => router.location())` says so. Data lives in
the screen (`createMemo(() => db.query(...))` under `<Loading>`, failures
in `<Errored>`); the router has no data layer.

`createLink(target)` gives `{ href, go }` for a pressable; styling is the
caller's.

## Back

`<Router>` registers one `onBack` step: it pops while the stack is deeper
than one entry, and at the root leaves the event to the platform default
(background on Android, exit elsewhere). It registers before anything it
renders, so a dialog or a screen that registers its own `onBack` gets the
event first, as before.

`useBlocker(fn)` guards leaving the current entry while the component is
mounted: back, navigate and replace ask it first. Return `false` to stay,
`true` to go, or a promise that decides later (a confirm dialog). A
blocked back is prevented and never falls through to the platform.

## Links in, location out

`createRouter` reads `env.launchLink`: a link the app was started with wins
over `initial` and lands as a stack of one. `<Router>` subscribes to
`onLink`: a link arriving while the app runs pushes onto the stack. A
custom-scheme link keeps everything after the scheme (`myapp://settings/theme`
is `/settings/theme`); an http(s) link drops its host; a query or fragment
is dropped. A link that matches nothing lands on the catch-all route, or is
ignored with a warning.

`<Router>` reports the current path through `reportLocation` from
`srt:dev`, so `GET /__control__/link` and the `get_location` MCP tool read
where the app is, and `POST /__control__/link` or `open_link` opens a
screen directly. `srt render --link /settings` renders that screen. A hot
reload starts the rebuilt app at the path it was on: the runtime hands the
last reported path to the new run as `env.launchLink`.

## Restore

The stack is plain data:

```tsx
onSuspend(() => file("nav.json").write(JSON.stringify(router.entries())))
```

To resume, read it back before creating the router and pass it as
`initial` (an array is a saved stack). Whether to resume is the app's call
(`env.launch === "restored"`); a launch link still wins.

## Reference

- `<Router initial?>` with `<Route path? component?>` / `<Route route>` children, or `<Router router />`; `<Outlet />`
- `createRoute({ path, params?, component?, children? })`, `createRootRoute({ component?, children? })`
- `createRouter({ tree, initial? })` returns `{ entries, location, navigate, back, href }`
- `useRouter()`, `useLocation()`, `useParams()` / `useParams(route)`, `useNavigate()`, `createLink(target, options?)`, `useBlocker(fn)`
- `matchPath(root, path)`, `formatPath(route, params)`, `linkToPath(link)` for tooling and tests
