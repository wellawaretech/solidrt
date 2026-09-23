---
title: Application routing
description: "A router for SolidRT apps: a declared, two-way mapping between a link string and the app's screen state, with the back stack, per-entry retention and layout routes built on it; the one feature that makes screens addressable by OS links, MCP, srt render, reload and restore alike. A layer above core's link primitive (deep-links.md), never inside it."
created: 2026-09-23
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
carries (address bar, forward, bookmarks, per-route code splitting) do not
apply here and are not part of this. That makes the router smaller than
its web namesake, not less useful.

## Vocabulary

Two words for two things, kept apart everywhere (core, router, CLI, MCP):

- **Link.** A string that arrives from outside and names a place in the
  app: `myapp://settings/theme` from the OS, `/settings/theme` from a
  tool. Both forms are accepted wherever a link is taken; the scheme and
  host, when present, identify the app and are dropped before routing.
  Core's inbound surface is named after it: `onLink`, `env.launchLink`
  (see [deep-links](deep-links.md)); the tooling is
  `srt render --link`, MCP `open_link`, `POST /__control__/link`.
- **Location.** Where the app is: the router's state, a path plus params,
  the top of the stack. Router API words are the web's (`navigate`,
  `location`, `route`, `params`) so that web developers and coding LLMs
  guess right; the semantics are SolidRT's.

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
   location saved in `onSuspend`). Logs and crash reports carry the
   location. With the router, `myapp://settings/theme` *is*
   `/settings/theme`; there is no per-app mapping at all. Given how
   central MCP, probes and `srt render` are to how this project is built
   and verified, this item alone justifies the feature.
2. **Back stack semantics done once.** `navigate` pushes, `back` pops,
   `replace` swaps; the router owns the `onBack` step for the stack, so a
   screen does not register a handler to get ordinary back behaviour
   (overlays and dialogs keep registering their own, above it). Includes
   the platform convention for a link into a detail screen: back goes to
   the list, not out of the app, from a parent stack the route tree can
   synthesize.
3. **Per-entry retention.** A pushed entry keeps the previous entry's
   scroll position and focused element and restores them on pop. Focus
   restore on back is what a TV user expects, and it is hard to hand-roll
   because the focused node has to be found again after a remount.
4. **Route-level preload.** Data fetching starts when navigation begins,
   not when the screen mounts. On TV and low-end devices that hides the
   latency; with Solid 2.0's `createAsync` and `<Loading>` the route can
   own the promise.
5. **Layout routes.** A shell around content, and adaptive list-detail
   (two routes shown side by side at wide widths, a stack at narrow
   widths, same route tree) expressed once instead of `NavShell` and
   `SplitView` plus selection state the app threads by hand.
6. **Transitions with direction** (push vs pop), and typed routes so a
   link to a removed screen is a compile error.
7. **Guards and redirects.** An auth gate that sends to login and returns
   to the intended location afterwards.

## Shape

A layer above core, opt-in: an app that does not use it loses nothing,
and core's link primitive works without it (primitive first; conveniences
on top; always a way not to use them).

- **Package.** Its own `@solidrt/router`, or a module of
  `@solidrt/components`; the latter is where `NavShell`, `SplitView`,
  focus navigation and the layout policy already live, and layout routes
  and focus restore need all of them. Decide when designing.
- **Route model.** Declarative routes with path patterns and params,
  nested for layouts (`<Router>`, `<Route path="/settings/:section">`,
  `useParams`, `useLocation`, `useNavigate`), matched against a path
  string. No `<a>`; a `Link` component is a pressable that navigates.
- **History.** Memory only, a stack: `navigate(to)` pushes,
  `navigate(to, { replace: true })` swaps, `back()` pops. No forward. The
  stack is data the app can read and serialize (`router.entries()`), and
  set (`router.restore(entries)`), which is how restore after a kill and
  reload re-entry work without the router knowing about either.
- **Back.** The router registers one `onBack` handler when mounted: pops
  if the stack has more than one entry, else leaves the event
  unprevented so the platform default runs (background on Android, exit
  elsewhere). Handlers registered later (a modal) still run first.
- **Links.** The router subscribes to core's `onLink` and reads
  `env.launchLink` at mount: strip scheme and host, match, navigate. The
  synthesized parent stack for a link into a nested route is a route
  option, not a default, so an app can choose "link lands with an empty
  stack" when that is right.
- **Location fact for tooling.** The router reports its location to core
  explicitly (`registerLocation(get, set)` or similar), and core exposes
  it through the control API: `/clients` shows it, `POST
  /__control__/link` delivers a link the same way the OS would. Nothing
  implicit: an app without a router shows no location and `link` still
  reaches `onLink` for the app to handle itself.
- **Retention.** Per-entry saved state: scroll offsets of scroll views in
  the entry, the focused node's id. Restored on pop. Off by default per
  route or on, to be decided with measurements of what remount costs.
- **Preload.** A route's `preload(params)` runs when navigation to it
  begins; its result is available to the screen through a hook. Same
  shape as Solid's router so the pattern is familiar, semantics reduced
  to "start early".

## Tooling that rides on it

- `srt render --link <link>`: render at a location. Screenshots for docs,
  visual regression per screen.
- MCP `open_link` and `POST /__control__/link`: open a screen in the
  running client. Also the way to test link handling on desktop and in
  the dev client without any OS registration, the analog of `adb shell
  am start -d` and `xcrun simctl openurl`.
- Reload re-entry: the dev server reads the location from the client
  before pushing a rebuild and delivers it as the launch link after.
- `/clients` and `get_logs` show the location.

## Rejected

- **No router, per-app parsing over the link primitive.** The first
  evaluation of this question (2026-09-23) concluded this, on the grounds
  that a browser router's features do not apply and the player routes by
  hand in a few lines. The second ground was circular: the player routes
  by hand because nothing else exists. The first was true but beside the
  point: the router's value here is not the browser features, it is the
  mapping that every tool and entry path shares. Without it, each of the
  six uses in item 1 needs the same parser written per app.
- **Adopting Solid's web router.** It has a memory history mode, but it
  is built for the DOM (`solid-js/web` imports, anchor handling, browser
  history) and its Solid 2.0 status is unclear. Nothing in it applies
  beyond the API names, which are taken for familiarity only. The router
  needed here is small; writing it is cheaper than bending that one.
- **Routing in core.** Core reports facts (`onLink`, `env.launchLink`,
  back); which screen a link means is app policy and lives above. Core
  gains only the explicit location registration so tooling can read it.
- **Locations that are not paths.** A typed-object location (an app
  enum) would need a per-app serializer for links and tooling, which is
  the problem the router solves. Paths with params are the serialization;
  typed route helpers can sit on top.

## Open questions

- Package placement (own package vs components), see Shape.
- Whether adaptive list-detail is a router concern (two routes, one
  layout) or stays with `SplitView` reading the location.
- Retention default and cost: measure a remount of a list screen on the
  low-end baseline before deciding.
- How the reload re-entry path talks to the client: the location is a
  fact core already reports through `/clients`, so the server can read it
  before the rebuild; the delivery half is the launch link.

## Depends on

- [deep-links](deep-links.md): core's `onLink` and
  `env.launchLink`, the OS registration in `srt pack`, and the control
  API `link` endpoint. The router is the main consumer but not a
  prerequisite for that item; the primitive ships first and alone.
