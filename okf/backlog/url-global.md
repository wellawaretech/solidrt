---
title: Add the WHATWG URL global
description: flux has no URL class, so new URL(...) throws "URL is not defined" and an app parses a relay URL or any URL by splitting strings; a thin standards_plugins marshal over the url crate forge already pins gives the standard shape.
created: 2026-11-16
---

# Add the WHATWG URL global

## Symptom

`new URL("http://token@host:1234/")` throws "URL is not defined". There is
no `URL` in flux/src/standards_plugins/ and none in
packages/flux-types/standards/. An app that has to take a `flux:p2p` relay
URL apart (the token in the user part, the port) does it with `split`.

## Shape

A `URL` global in `standards_plugins/` (a web standard goes there), with
the standard constructor (absolute, and relative with a base), `href`,
`origin`, `protocol`, `username`, `password`, `host`, `hostname`, `port`,
`pathname`, `search`, `hash`, `toString`, `toJSON`, and `URL.canParse`.
`searchParams` (`URLSearchParams`) is a second step: it is the larger
surface and the one that may not be needed.

Backing: the `url` crate (forge already pins `=2.5.8`), which is the
WHATWG algorithm, so the semantics come for free; the plugin only marshals.
Parse errors throw a `TypeError` as the standard says.

Through the solidrt lens nothing simplifies here: the standard contract is
the useful one.

## Done looks like

`new URL` works for the cases `flux:p2p` and `flux:http` hand out,
`flux/tests/url.test.ts` covers parse, components, relative resolution and
the error case, and packages/flux-types/standards/url.d.ts declares it.
