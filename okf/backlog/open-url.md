---
title: Open a link in another app
description: "An app cannot open an external link (a store page, a credits link, a mailto:, a text share link): onLink only receives links; wanted an openUrl(url) backed by SDL_OpenURL, which the sdl3 crate already wraps and SDLActivity already implements on Android."
created: 2026-11-03
---

# Open a link in another app

An app cannot send the user out to a link. `onLink` (see
[deep-links](deep-links.md)) only receives links; nothing opens one. The
common needs are small: a store page to rate the app, a website from a
credits screen, a `mailto:` for feedback, a messenger's text-share link.

## What done looks like

One call that hands a link to the platform, which opens it in the app
registered for its scheme (the browser for `https:`, the mail app for
`mailto:`, and so on).

```ts
await openUrl("https://example.com/credits")
```

It resolves once the platform has accepted the link. It rejects when the
platform has no handler or refuses it. It says nothing about what the
other app does with the link.

## What it involves

- `sdl3::url::open_url` wraps `SDL_OpenURL`, so no new wrapper is needed.
  On Android, SDL implements it in `SDLActivity.openURL` (an
  `ACTION_VIEW` intent); on desktop it uses the platform opener
  (`xdg-open`, `open`, `ShellExecute`).
- SDL is needed, so the call lives in alloy and an `alloy_plugins/`
  module marshals it.
- An entry in `Flux.capabilities` only where it works (not headless).
  A call elsewhere rejects with `NotSupportedError`, as in
  [flux:dialog](../../packages/flux-types/gui/dialog.d.ts).
- Under `sol test` and the control API, the call is recorded instead of
  opening a browser, so a test or an agent can assert which link the app
  opened.

## Open questions

- Name and home: `openUrl` in a `flux:` module, or core re-exports it
  next to `onLink`. The web has no standard call for this (`window.open`
  is a browsing-context API), so the web-shape rule does not decide it.
- Scheme validation: accept any scheme, or reject a link with no scheme
  in dev.
