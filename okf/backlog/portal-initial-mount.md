---
type: backlog-item
title: Portals cannot mount during the initial render
status: done
timestamp: 2026-07-17T00:00:00Z
---

# Portals cannot mount during the initial render

Found 2026-07-17 while smoke-testing createPortal: a portal-backed component
that is visible at first mount (for example `<Modal>` not gated behind a
`<Show>` that starts false) throws "no mount target". render() only learns
the window root after the app's `code()` returns, but the JSX inside builds
eagerly, so any portal created during that first build sees no target.

**Decision (user, 2026-07-17): this is the contract, not a bug.** A portal at
first render is an exception, not the rule; portal content is overlay
content, opened by a signal that starts false. Documented in createPortal's
doc comment and Modal's doc comment; the error message now says "portals
cannot mount during the initial render; open them after mount". No parking
or deferred-flush mechanism will be built.

Residual note: the throw still lands inside reactive scopes (Show/memo), so
until okf/plans/reactivity-halt-containment.md is resolved it surfaces as an
uncaught error like any other component throw.