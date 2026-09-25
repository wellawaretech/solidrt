---
title: Application routing
description: "@solidrt/router: a typed route tree with validated params, a memory stack run as Solid transitions, one back step, blocking, links in and location out; the mapping that makes screens addressable by OS links, MCP, srt render, reload and restore alike. Own package on core only, headless core plus a thin Solid binding, no data layer. A consumer of the link primitive in deep-links.md, never inside core."
created: 2026-09-23
completed: 2026-09-23
---

# Application routing

Every SolidRT app with more than one screen keeps its navigation state in
signals of its own shape (`screen`, `panel`, `selectedId`), switches on
them, and registers one `onBack` handler per level. That works, and stays
the right answer for an app that wants nothing more. But it means no one
outside the app can name a screen: not a deep link, not an MCP tool, not
`srt render`, not the dev server after a reload, not the app itself when
it restores after Android killed it. Each of those needs a string that
means "the settings screen, theme section", and the app would have to
write the parser and the serializer for it, twice, and keep them in step.

A router is that mapping, declared once. Everything below follows from
making screens addressable; the browser features a web router also
carries (address bar, forward, bookmarks, per-route code splitting, data
loaders and preloading) do not apply here and are not part of this. That
makes the router smaller than its web namesake, not less useful.

## Vocabulary

Two words for two things, kept apart everywhere (core, router, CLI, MCP):

- **Link.** A string that arrives from outside and names a place in the
  app: `myapp://settings/theme` from the OS, `/settings/theme` from a
  tool. Both forms are accepted wherever a link is taken; the scheme and
  host, when present, identify the app and are dropped before routing.
  Core's inbound surface is named after it: `onLink`, `env.launchLink`
  (see [deep-links](../backlog/deep-links.md)); the tooling is `srt render --link`,
  MCP `open_link`, `POST /__control__/link`.
- **Location.** Where the app is: the router's state, a path plus parsed
  params, the top of the stack. Router API words are the web's
  (`navigate`, `location`, `route`, `params`) so that web developers and
  coding LLMs guess right; the semantics are SolidRT's.

Not "URL": it over-promises (a location is not a URL), and every platform
calls the inbound thing a link already (Android deep links and App Links,
Apple Universal Links, Expo's linking module).

## What it brings

In order of value to this project:

1. **Screens addressable by everyone, with one vocabulary.** A link opens
   the screen on every entry path: the OS (deep link, notification, app
   shortcut, Android TV channel), `srt render --link /settings` for a
   deterministic screenshot of any screen, MCP `open_link` so an agent
   verifies a screen without tapping through `/tree`, reload-on-save
   re-entering the location the developer was looking at, and restore
   after the system killed the app (`env.launch === "restored"` plus the
   stack saved in `onSuspend`). Logs and crash reports carry the
   location. With the router, `myapp://settings/theme` *is*
   `/settings/theme`; there is no per-app mapping at all. Given how
   central MCP, probes and `srt render` are to how this project is built
   and verified, this item alone justifies the feature.
2. **Compile-time links.** Routes are typed values, so `navigate` to a
   removed screen or with a wrong param is a build error. An app shipped
   as bytecode never sees a 404; the type checker is the only net.
3. **Validation at the boundary.** A link is untrusted input. A route's
   `parse` is the one place raw strings become typed values, and a
   component can only read what passed it. This is how the "make the
   untrusted nature obvious at the call site" requirement of the
   deep-links item is met by structure rather than by a doc comment.
4. **Back stack semantics done once.** `navigate` pushes, `back` pops,
   `replace` swaps; the router owns the `onBack` step for the stack, so a
   screen does not register a handler to get ordinary back behaviour
   (overlays and dialogs keep registering their own, above it). Includes
   the platform convention for a link into a detail screen: back goes to
   the list, not out of the app, from a parent stack the tree can
   synthesize.
5. **Blocking.** An unsaved-changes guard holds a pop or a navigation
   until the user answers; a blocked back is prevented so `exit()` does
   not run. The one guard native apps need, and it composes with the
   existing `onBack` and `exit()` contract.
6. **Per-entry retention.** A pushed entry keeps the previous entry's
   scroll position and focused element and restores them on pop. Focus
   restore on back is what a TV user expects, and it is hard to hand-roll
   because the focused node has to be found again after a remount.
7. **Navigation as a transition.** The old screen stays visible until the
   new one has what it needs; `isPending` drives the indicator. This is
   Solid 2.0 behaviour, the router only has to run navigation inside a
   transition; it is listed because it is why no data layer is needed.

## Where the line is

Core is unopinionated: it reports facts and offers mechanisms, and the
router is built on them like any other app code could be. The test for a
change: if it only makes sense because a router exists, it does not go in
core. Concretely:

Core gets, and nothing more:

- `onLink(fn)`: a link arrived, the raw string, untouched. No scheme or
  host stripping, no matching, no validation; that is the consumer's.
- `env.launchLink`: the link the process was started with, raw, or null.
- `reportLocation(value)`: a slot where an app may report a location
  string for tooling to read. The runtime stores the string and answers
  `GET /__control__/link` (`get_location`) with it, and hands it to a
  reload of the same app as its launch link; it never interprets it, and
  it is not tied to the router (an app routing by hand may report one
  too).
- The control API `link` endpoint: emits the raw link on the bus, exactly
  as the OS path does. No routing knowledge in the CLI or MCP either.

Everything the router uses beyond that already exists for everyone:
`onBack`, `setFocus`, `exit()`, `env.launch`, Solid transitions.

The router owns every opinion: the route tree and its syntax, `parse`,
the stack and its back step, blocking, what a launch link does to the
stack, precedence of a link over initial entries, not-found handling,
and the warning on an unmatched link. None of these leak into core as a
default, a flag or a helper. If core seems to need one, the design is
wrong and the router adapts.

## Shape

`@solidrt/router`, its own package in `packages/router`, depending on
`@solidrt/core` only (`workspace:*`, version `0.0.0` like the others).
Routing is independent of components: components is a design system, and
an app with its own design system still needs routing. What the router
needs from the UI is thin and stays on its side: link handlers, and a
per-entry store that `ScrollView` opts into.

Two layers in one package:

- `src/route.ts` **headless**: route tree, matching, parsing, path
  formatting, link normalization. Plain functions and data, no rendering,
  unit-tested without a client (`tests/route.test.ts`).
- `src/router.tsx` the **Solid 2.0 binding**: `createRouter` (the stack,
  blocking), `Router`, `Outlet`, `useRouter`, `useNavigate`,
  `useLocation`, `useParams`, `createLink`, `useBlocker`.

### Route tree

Two forms over one headless tree (decided 2026-09-23, after the first
cut had values only): JSX for readability, values where a param needs a
type.

```tsx
let section = createRoute({
  path: "/$section",
  params: { parse: (raw) => ({ section: parseSection(raw.section) }) },
  component: Section,
})

<Router initial="/">
  <Route path="/" component={Shell}>
    <Route path="/" component={Home} />
    <Route path="/settings" component={SettingsLayout}>
      <Route route={section} />
    </Route>
    <Route path="/$" component={NotFound} />
  </Route>
</Router>
```

A `<Route>` evaluates to a route value; `<Router>` resolves its children
with `children()` and places them under a root of its own, once, at mount
(the way Solid's router builds its tree). A route value is pure: no
`parent:` option, no mutation at creation; placement (a `children:` list
on `createRoute`/`createRootRoute`, or a JSX `<Route route={x}>`) sets the
parent, and a value has one place in one tree. This dropped the
import-order trap the value-only form had (a route mutating its parent at
module scope), and the inherited-params generic: `useParams(route)` types
the route's own params, an ancestor's are read through the ancestor. A
JSX `<Route>` takes `path` and `component` only, no `params`: one way to a
typed param, and it is the one that types the read. The router of a JSX
tree lives inside `<Router>` and is reached through the hooks; a value
tree can be made at module scope (`createRouter({ tree })`,
`<Router router={r}>`), which the player does.

Targets are route values or paths: `navigate(home)` for a route without
params, `navigate({ route: section, params: { section: "theme" } })` for
one with, and a wrong route or a missing param fails to compile (a `Route`
with required params is not assignable to the bare form).
`navigate("/settings/theme")` takes a path as a link or a tool would carry
it, matched at run time. `useParams(section)()` is typed by the route,
`useParams()()` is the screen's raw strings. Path syntax:
literal segments, `$param`, `$param?`, `$` for the rest. `$` rather than
`:` (decided 2026-09-23): it cannot be confused with the `:` in a
`myapp://...` link, and it is what typed route trees use. The path is the
whole location; there are no search params (see Rejected). A layout
route renders its child through `<Outlet>`. A failed `parse` is a
non-match, so the path falls to the next sibling and then to a catch-all
`$` route if the tree has one last under the root: that route is the
not-found screen, no separate option.

### Stack, back, blocking

- Memory stack. `navigate(to)` pushes, `navigate(to, { replace: true })`
  swaps, `navigate(to, { reset: true })` starts the stack over at it (a
  flow finished: a scan dialed, a login done; added for the player),
  `router.back()` pops. No forward. Every navigation is a signal write,
  so Solid's transition semantics apply.
- `Router` registers one `onBack` at mount: pops while the stack is deeper
  than one entry, else leaves the event unprevented so the platform
  default runs (background on Android, exit elsewhere). Handlers
  registered later (a modal) still run first.
- `useBlocker(fn)`: any pop or navigation away from the current entry
  asks the guard first; the guard can hold the navigation and resolve it
  later (the dialog). A blocked back is prevented.
- `router.entries()` returns the stack as plain data, and
  `createRouter({ initial })` takes a path or such an array. That is the
  whole restore story: `onSuspend` saves `entries()`, a restored launch
  reads them back (under `<Loading>`, it is async) and mounts the router
  with them as `initial`. No `restore()` call after mount, so nothing
  races the launch link; when both a launch link and initial entries
  exist, the link wins, the user just asked for that screen.

### Links in, location out

- `Router` reads `env.launchLink` at mount and subscribes to `onLink`:
  strip scheme and host, match, parse, navigate. A launch link lands with
  a stack of one entry; a warm link pushes onto the current stack. A
  link that matches no route (or fails `parse`) lands on the tree's
  `notFound` route when there is one, else is ignored with a warning and
  the app stays put: a link is external input, not API misuse, so it is
  not a throw-in-dev site.
- `Router` calls `reportLocation(path)` from `srt:dev` (typed in core's
  runtime-modules.d.ts) on every change of the current path, nothing
  more, and `reportLocation(null)` on unmount. A reported value, not a
  getter: the runtime keeps a plain string that outlives the engine, so
  the dev connection answers `GET /__control__/link` without a JS-thread
  round trip and a reload can carry it over. An app without a router
  reports no location, and `link` still reaches `onLink` for the app to
  handle itself.

### Later, not in the first version

Cut 2026-09-23 as conveniences on top of a router that is complete
without them; each is additive and has its own backlog item:
[router-per-entry-retention](../backlog/router-per-entry-retention.md)
(scroll and focus lost when back remounts a screen) and
[router-link-parent-stack](../backlog/router-link-parent-stack.md) (a
link into a nested screen outside tabs has nothing beneath it; done for
tab screens by [router-tabs](router-tabs.md)).

### Data

None. A screen loads what it shows the way any SolidRT component does:
an async computation read under `<Loading>`, failures caught by
`<Errored>`. Because navigation is a transition, the previous screen
stays until those reads settle, nested screens load in parallel as
independent computations, and `isPending(location)` is the indicator. No
loaders, no loader cache, nothing to invalidate.

### Ships with

- The player migrated to it: list-detail at two widths, the scan screen's
  back step, the settings and connect panels. The one real multi-screen
  app, so the API is exercised before anyone else uses it.
- `packages/router/docs.md`, and a pointer from the components docs for
  styling a link with `createLink`.

## Tooling that rides on it

- `srt render --link <link>`: render at a location. Screenshots for docs,
  visual regression per screen.
- MCP `open_link` and `POST /__control__/link`: open a screen in the
  running client. Also the way to test link handling on desktop and in
  the dev client without any OS registration, the analog of `adb shell
  am start -d` and `xcrun simctl openurl`.
- Reload re-entry: the client itself, not the server. When an engine ends
  the runtime keeps the location its app reported, and the next run of
  the same app (a rebuild push; a build failure's BSOD in between reports
  nothing and keeps it) starts with that location as `env.launchLink`, so
  the rebuild comes back to the screen it left. A different app (a player
  launch, the player after a stop) starts from its own beginning. No
  protocol change, no round trip before a push, nothing for the reload
  composers to do.
- `/clients` and `get_logs` show the location.

## Stages

1. DONE 2026-09-23: the link primitive and dev injection
   ([deep-links](../backlog/deep-links.md), its "what it involves" item
   5) together with the router (`packages/router`) and the player
   migration (`apps/player/src/routes.ts`, `parts/app-state.ts`).
   Verified live on a file-mode dev server: links land on every player
   screen through `POST /__control__/link`, `GET` reads the location, a
   synthetic gamepad's back pops the stack, a bad id is refused with a
   warning, and `srt render --link` renders a probe at a link.
2. DONE 2026-09-23: reload re-entry, in the client's engine loop (see
   Tooling). Done with it: `registerLocation(getter)` became
   `reportLocation(value)`, since a value the runtime holds can be read
   after the engine that reported it is gone, a getter cannot. Verified
   live on file-mode servers: the player opened at `/settings` by link
   comes back at `/settings` after `POST /__control__/reload` (location
   and tree), and the router probe at `/settings/theme` survives a saved
   syntax error (BSOD, location null) and returns to `/settings/theme`
   when the fix is saved.
3. DONE 2026-09-23 for Android, Linux and Windows: OS registration, its
   own item in the [deep-links](../backlog/deep-links.md) note (the
   scheme is the appId; `registerProtocolHandler()`; the desktop
   single-instance hand-off). macOS (an `.app` bundle) stays open there.

## Findings

Cut into
[routing-and-link-delivery-traps](../notes/routing-and-link-delivery-traps.md):
the engine's exec drain before module evaluation, SDL's drop event as the
URL delivery path, the per-row `STRICT_READ_UNTRACKED` pattern, and the
list-detail answer over a layout route. Two bugs met on the way predate
routing and are filed on their own:
[segmented-control-strict-read](../backlog/segmented-control-strict-read.md)
and
[player-headless-render-exits](../backlog/player-headless-render-exits.md).

## Rejected

- **No router, per-app parsing over the link primitive.** The first
  evaluation of this question (2026-09-23) concluded this, on the grounds
  that a browser router's features do not apply and the player routes by
  hand in a few lines. The second ground was circular: the player routes
  by hand because nothing else exists. The first was true but beside the
  point: the router's value here is not the browser features, it is the
  mapping that every tool and entry path shares. Without it, each of the
  uses in item 1 needs the same parser written per app.
- **JSX route declarations only** (`<Route path="...">`, the shape of
  Solid's router, with nothing else). Routes as untyped strings are the
  pattern routers moved away from (TanStack Router, Vue Router 4.4, Expo
  Router): a typed tree is the only thing that makes a link to a removed
  screen a build error. Rejected at first in full; revised 2026-09-23 to
  JSX plus route values (see Route tree): the JSX carries the readable
  tree, a value carries a typed param, and the two place into one tree.
- **A value-only tree with `parent:` back-references** (the first cut).
  Assembled by side effect, in call order, with an import-order trap for
  the module that declares it; and unfamiliar next to every JSX router.
- **Route loaders and preload on intent.** Proposed as the current web
  practice (loaders per route run before the swap, preload on hover, or
  on focus for TV). Preload on the web hides fetching the route's code
  chunk and its data; here the code is in the bundle, and the loader
  with the old screen staying visible already covers the data wait, so
  preload only shaved the moment between press and screen at the cost of
  a keyed loader cache with invalidation and staleness rules. With
  preload gone, loaders lose their reason too: Solid 2.0 transitions give
  "old screen stays until the new one is ready" for the screen's own
  async reads. Data stays in the screen component.
- **Routing inside `@solidrt/components`.** Convenient (focus-nav,
  ScrollView and the layout policy are there) and wrong: it couples the
  design system to navigation, and breaks the rule that components runs
  independently. The router needs nothing from components; components
  opts into the router's per-entry store.
- **Adopting a web router package.** Solid's router is built for the DOM
  and its Solid 2.0 status is unclear; TanStack's framework-agnostic core
  has a memory history but carries web assumptions (search-param
  serialization, location shape, document APIs) and a Solid 1.x binding.
  The patterns are taken (typed tree, validation, blocking, headless core
  plus binding); the packages are not. The router needed here is a few
  hundred lines.
- **Routing in core.** Core reports facts (`onLink`, `env.launchLink`,
  back); which screen a link means is app policy and lives above. Core
  gains only the explicit location registration so tooling can read it.
- **Locations that are not paths.** A typed-object location (an app
  enum) would need a per-app serializer for links and tooling, which is
  the problem the router solves. Paths with params are the serialization;
  the typed tree sits on top.
- **Search params** (`?tab=advanced`, `useSearch`, a second typed and
  validated surface next to params). Dropped 2026-09-23: a path param
  carries the same value (`/settings/theme/advanced`), and one location
  grammar keeps the tree, the links and the tooling simpler.
- **File-based routes, route masking, actions and forms, history beyond
  the stack, code splitting.** Web concerns, or implicit behaviour.

## Open questions

- None for the first version; see Findings for the list-detail answer.

## Depends on

- [deep-links](../backlog/deep-links.md): core's `onLink` and `env.launchLink`, the
  control API `link` endpoint, and later the OS registration in `srt
  pack`. The router is the main consumer but not a prerequisite for that
  item; the primitive is usable alone.
