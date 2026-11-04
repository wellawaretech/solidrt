---
title: ore landing zone - the OS layer crate, the link facility first
description: "Start the platform-layering design (okf/design/platform-layering.md): create the `ore` crate with the main-thread bridge and its first facilities (openLink on SDL_OpenURL, the locale, the scheme registration and single-instance hand-off moved from lattice), marshal them through a new flux `ore_plugins/` layer (`flux:link` with openLink and registerProtocolHandler), re-export them from core next to onLink, rewrite the placement rule in all six places in the same change, and rename the MCP open_link tool to send_link; existing code moves later, as features touch it."
created: 2026-11-03
---

# ore landing zone

## Problem

The placement rule defined alloy and forge by SDL ("alloy is what needs
SDL", "forge never goes through SDL"), so every OS feature reopened the
same question, and work stalled or landed in the wrong place. The design
is settled in okf/design/platform-layering.md: our own OS layer, `ore`,
is the platform boundary, wraps SDL where SDL reaches and our own native
code where it stops. Read that note first.

Until `ore` exists and the rules name it, new OS work still lands in the
old places and adds debt. This plan creates the landing zone; it does not
move existing code.

The first facility is the one that exposed the problem: an app cannot
send the user out to a link. `onLink` only receives links. The common
needs are small: a store page to rate the app, a website from a credits
screen, a `mailto:` for feedback, a messenger's text-share link.

## Done looks like

- An `ore` crate in the workspace, under alloy and forge
  (okf/design/platform-layering.md, "The layers"), with the main-thread
  bridge, `openLink` and the locale.
- `openLink` in app code, from `@solidrt/core`, next to `onLink`:

  ```ts
  await openLink("https://example.com/credits")
  ```

  It resolves once the platform has accepted the link. It rejects when the
  platform has no handler or refuses it. It says nothing about what the
  other app does with the link.
- The locale reachable from JS (`navigator.languages`).
- The scheme registration and the single-instance hand-off in
  `ore::link`, `registerProtocolHandler` in `flux:link` (core re-exports
  it, as before), the app's identity handed to the engine as a plain
  value.
- Under `sol test` and the control API, `openLink` is recorded instead of
  opening anything, so a test or an agent can assert which link the app
  opened.
- The placement rule rewritten in all six places, naming `ore`.
- The MCP tool `open_link` renamed to `send_link`.

## Decisions

- **Name: `openLink`.** The repo's "link" vocabulary stays (`onLink`,
  `env.launchLink`, the router's `createLink`): a link is what you follow,
  a URL is its string form. Peers do not settle direction with the noun
  either (React Native's `openURL` goes out, Electron's `open-url` comes
  in); the verb does. `openUrl` was the alternative, rejected for breaking
  the vocabulary. The web has no standard call for this (`window.open` is
  a browsing-context API).
- **It always goes out to the platform.** Its doc comment says so: a link
  of the app's own scheme comes back in through `onLink`; in-app
  navigation is the router's `createLink(...).go()`.
- **`send_link` for the MCP tool.** `open_link` was named from the agent's
  side but delivers a link into the app, the opposite of `openLink`.
  `send_link` sits with `send_input` and says the direction. The control
  endpoint (`/__control__/link`) keeps its path.
- **Backed by `SDL_OpenURL`** through the `sdl3::url::open_url` wrapper.
  On Android, SDL implements it in `SDLActivity.openURL` (an `ACTION_VIEW`
  intent through the activity); on desktop it uses the platform opener
  (`xdg-open`, `open`, `ShellExecute`). It is main-thread only, so it runs
  through the bridge.
- **The locale** is a web standard, so it is marshalled in
  `standards_plugins/` (the root CLAUDE.md rule), backed by `ore`
  (`SDL_GetPreferredLocales`, not thread safe: one caller at a time behind
  a lock). A change event (`languagechange`) needs SDL's locale event from
  the pump, which moves to `ore` later; read on access until then.
- **SDL init stays in alloy for now.** `ore`'s first facilities are
  window-free calls that work without alloy's context. Headless `flux`
  runs JS on its main thread, so a main-thread call runs there directly.
- **Module `flux:link`** (2026-11-04). It holds `openLink` and
  `registerProtocolHandler`; the inbound side stays the `link` bus event.
  `flux:shell` read as a subprocess, `flux:open` as a bare verb.
- **The scheme registration moves now** (2026-11-04), not when a feature
  touches it: it is the other half of the link facility, and `openLink`
  already gave ore its link session. All of `lattice/src/links.rs` is OS
  code, so all of it goes to `ore::link`: `register` (the freedesktop
  entry, the Windows protocol key; Android's package and macOS's bundle
  declare the scheme), `own_link`, and the single-instance `hand_off` and
  `listen`. `listen` hands each link to a callback as a plain string;
  lattice turns it into `AlloyEvent::Link` and raises the window. The
  endpoint dir stays a parameter: the pref path is the storage roots' move.
  `windows-registry` moves with it, and ore takes `tokio` for the listener.
- **The app's identity is a plain value the host hands the engine.** The
  link session (`ore::link::Session`, renamed from `Opener`) holds an
  `OwnScheme`: `Unknown` (no app identity: headless flux, a dev run of the
  runner; the call throws), `NotHandler` (the dev client and the player:
  the call warns and returns, so startup code runs unchanged) or `Own`
  (a packed app: the call writes the registration). lattice picks the
  variant where it built the `sol:app` closure before, through
  `flux::link::set_own_scheme`; ore decides what the platform does with
  `Own`; flux marshals. A record-only session writes nothing and returns.
  The hand-off stays a host call: lattice calls ore before start and to
  listen. `sol:app` keeps `exit` and `background`.
- **No read-back and no capability for the registration.** A test does not
  need to assert it yet, and the call throws with the reason where it
  cannot register; only packed desktop apps call it.
- **A link needs a scheme** (RFC 3986 grammar), any scheme of two or
  more characters: `openLink` throws a TypeError on `/settings` or
  `example.com`, the likely mistakes, and on a Windows drive path
  (`C:\file`), which the opener would run as a local file.
- **The log holds every link the app asked to open**, refused ones too
  (`requestedLinks`, `get_requested_links`): it says which link a button
  tried, whatever the platform answered.
- **The capability is probed once**, as the dialogs are: `link` is listed
  where the platform has an opener (always on Android, Windows, macOS; on
  Linux the portal service or `xdg-open` on the path).
- **The dev client opens and records.** Every engine logs the links it
  opened (the last 64); only a stepped engine (test, render) and the
  `flux --test` host record without opening. A human using a dev client
  driven by an agent still gets the browser.
- **Not in scope:** moving the pump, the window, the audio device, the
  camera, the Android glue, the pref path or forge's OS modules. They move
  when a feature touches them (the design note, "Migration").

## Work

1. **Spike: the main-thread bridge.** Confirm in a throwaway that SDL's
   event wait runs `SDL_RunOnMainThread` callbacks, that queuing one wakes
   a blocked `wait_event_timeout_ms`, and that `sdl3::sys` exposes it. The
   bridge's design depends on the answer; if a blocked wait does not wake,
   the bridge pushes a wake event after queuing.
2. **The `ore` crate.** Manifest with the exact `sdl3 =0.19.0` pin and the
   per-target features alloy uses; the bridge (`run on main`, completing a
   promise-friendly channel); `open_link(&str) -> Result<(), String>`;
   `preferred_locales() -> Vec<String>`. Tests in `src/tests/`.
3. **flux.** `flux` depends on `ore` (headless included: about 4 MB and an
   SDL build, measured in the design note). A new `ore_plugins/` layer
   marshals `openLink` into a `flux:` module; the module name is still to
   choose (`flux:link` reads as also holding the inbound side, which stays
   in lattice; `flux:open` and `flux:shell` are the other candidates).
   `navigator.languages` in `standards_plugins/`. A `Flux.capabilities`
   entry for `openLink` only where it works; elsewhere the call rejects
   with `NotSupportedError`, as `flux:dialog` does.
4. **core.** `openLink` re-exported next to `onLink` in
   `packages/core/src/window.ts`, with the contract above; its types in
   `packages/flux-types`.
5. **Recording under test and control.** The host swaps in a recording
   opener: `sol test` and the control API record the link instead of
   opening it. A read for each: `@solidrt/test` (next to its `link` and
   `dialog` verbs) and a control endpoint with an MCP tool.
6. **The rule rewrite**, in the same change as the crate:
   - root CLAUDE.md: Projects (an `ore` entry; alloy and forge entries),
     the placement paragraph (the four-step order from the design note),
     the SDL section (wrappers go in `ore`, not `alloy/src/sdl_utils.rs`),
     the flux plugin layers paragraph (`ore_plugins/`);
   - alloy/CLAUDE.md: the opening and "What belongs here";
   - forge/src/lib.rs: the module doc comment;
   - flux/CLAUDE.md: the plugin layers;
   - `ore/CLAUDE.md`, new;
   - the platform-layering memory, rule 6.
7. **`send_link`.** The tool in `packages/cli/src/mcp/main.ts` and every
   mention: the `settle` and `get_location` descriptions,
   `packages/cli/src/mcp/docs.md`, `packages/core/src/window.ts` (the
   `onLink` comment), `packages/router/AGENTS.md`,
   `packages/router/README.md`. A running bridge picks it up only after a
   re-spawn.
8. **CI.** Check that every target the `flux` binary builds for can build
   SDL from source, and the build time it adds.
9. **The scheme registration to ore.** `lattice/src/links.rs` and its
   tests to `ore::link` (`register`, `own_link`, `hand_off`, `listen`);
   `Opener` renamed `Session`, with the `OwnScheme` and `register_handler`;
   `registerProtocolHandler` in `flux:link`, core re-exporting it from
   there, out of `sol:app` and its types; lattice sets the `OwnScheme` and
   calls ore for the hand-off. Then the notes: the design note ("app
   identity", the lattice inventory), okf/backlog/deep-links.md, the pack
   docs and the lattice comments that name `links.rs`.

## Findings

- The spike (work step 1) holds without a wake push: the result is in
  okf/design/platform-layering.md, "The main loop", and on
  `ore::main_thread`. The callback runs inside the blocked wait, which
  does not return for it.
- `SDL_OpenURL` on Linux tries the portal's OpenURI, then spawns
  `xdg-open` in the background and reports success once it started, so a
  link nothing handles resolves there. Android, Windows and macOS report
  it. The contract says so (`ore/src/link.rs`, `flux:link`'s types).
- `SDL_GetPreferredLocales` lists a locale twice when `LANGUAGE` and
  `LANG` both name it (`de_DE:fr` with `de_DE.UTF-8` gave `de-DE, de-DE,
  fr`); ore drops the repeat.
- With a `C` locale SDL reports none; `navigator.languages` falls back to
  `["en-US"]`.
- On Android, `SDL_OpenURL` returns false without setting an error when
  the activity's `openURL` finds no app for the link, so
  `SDL_GetError()` returned a stale message from an earlier call ("That
  operation is not supported"). ore clears the error before the call and
  reports a refusal with none set as "no app on this device handles the
  link" (`open_now`, through `sdl3::sys`, not `sdl3::url::open_url`).
- Verified on the SM-T500 (Android 12): `https:` brings Firefox to the
  front and resolves, an unhandled scheme rejects with the app staying in
  front, `navigator.languages` reads `["en-US"]`. Desktop Linux opens the
  browser (checked by hand).
- `sdl3::get_error()` unwraps the message as UTF-8 and panics on anything
  else; ore reads `SDL_GetError` itself (`ore::sdl_error`).
- The control API's 404 list (`CONTROL_ENDPOINTS` in control.ts) lacked
  `/__control__/dialog`; added with `/__control__/requested-links`.
