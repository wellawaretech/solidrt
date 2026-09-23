---
title: Deep links
description: "Opening the app at a link from outside: the app half (onLink, the launch link, the dev-side link injection) and the OS registration half (the scheme is the appId; the Android manifest, desktop self-registration through registerProtocolHandler, the desktop single-instance hand-off), both built 2026-09-23 for Android, Linux and Windows; what remains is macOS's .app bundle and the player's own scheme."
created: 2026-07-26
---

# Deep links

An external source hands the app a link and the app opens at the place
that link names: a link in a browser, a QR code, another app, a
notification. Expo's `expo-linking` is the closest reference for what app
developers expect to be there.

Raised 2026-07-26 while writing the website's Core concepts, where the
absence would otherwise have to be explained. Deliberately NOT flagged on
the website (no "maybe we will implement this" notes there); this file is
the record. Re-evaluated 2026-09-23 together with
[app-routing](../plans/app-routing.md), which is the consumer that turns a link
into a screen; this item is the primitive underneath and ships first.

## Vocabulary

**Link**, not URL: a link is `myapp://settings/theme` from the OS or
`/settings/theme` from a tool, and both are accepted everywhere a link is
taken. The name matches every platform's own term (Android deep links and
App Links, Apple Universal Links, Expo linking). So: `onLink`,
`env.launchLink`, `srt render --link`, MCP `open_link`,
`POST /__control__/link`. The router's internal state is a *location*;
see the routing item.

## It splits in two, and only one half is hard

**The app half is one event carrying a link.** `onLink((link) => ...)`
next to `onBack` in core's window module, and `env.launchLink` for the
link the app was started with. No router, no history, no location object
in core; what the link means for the app's screen state is the app's
policy, and [app-routing](../plans/app-routing.md) is the layer that does it
declaratively for apps that want that.

Cold start needs care: the launch link exists before the app's first
frame, so it has to be readable by a listener registered during the first
render rather than fired and lost. The sticky-event mechanism behind
`env.visibility` and `env.launch` (forge::events StickyCache) is the
precedent; on Android and desktop the link can ride argv next to
`--restored`, on macOS it arrives as an event after launch, so the sticky
event is the one uniform path.

**The OS half is registration**, per platform, and it is packaging work
rather than runtime work. `srt pack` (and the manifest it writes) grows a
field for the scheme the app answers to.

## Per-platform facts (verified 2026-09-23 against SDL 3.4.10)

| Platform | Registration | Already running | State (2026-09-23) |
| --- | --- | --- | --- |
| Android | VIEW intent filter with `<data android:scheme>` | `onNewIntent` (manifest is `singleInstance`) | DONE. The runner's prod manifest declares the filter with the scheme spelled as the package name, so the APK pack's package rewrite renames the scheme with it (one pool string). `SolidRTActivity` puts a cold intent's data on argv as `--link` and forwards a warm one through SDL's `onNativeDropFile`. Tablet-verified with implicit VIEW intents, cold and warm. |
| macOS | `CFBundleURLTypes` in a `.app` Info.plist | Same Apple Event, sent to the running instance (LaunchServices keeps one) | Open. SDL delivers `kAEGetURL` as `SDL_EVENT_DROP_FILE` carrying the link string; alloy maps it. `srt pack` builds no `.app`; and a cold link arrives as an event after launch, so the runtime will have to treat a link before the first frame as the launch link. |
| iOS | `CFBundleURLTypes` | `openURLContexts` on the scene | SDL delivers it as a drop file with `absoluteString`; alloy maps it. No iOS target yet. |
| Windows | HKCU `Software\Classes\<scheme>` with `URL Protocol` and `shell\open\command "%1"` | Starts a second process | DONE. `registerProtocolHandler()` writes the key (windows-registry crate); the packed binary takes a first argument of its own scheme as the launch link; a second instance hands the link to the first over a named pipe and exits. Box-verified through `start <scheme>://...`, cold and warm. The shell normalizes `scheme://word` to `scheme://word/` on the way; the router drops empty segments, so it matches, but the reported location keeps the slash. |
| Linux | `.desktop` with `MimeType=x-scheme-handler/<scheme>`, `Exec ... %u`, `xdg-mime default` | Starts a second process | DONE. `registerProtocolHandler()` writes the desktop entry and the mimeapps.list default directly (what `xdg-mime default` writes, without depending on xdg-utils); hand-off over a Unix domain socket in the app's client dir. Verified through `xdg-open`, cold and warm. |

Platform differences are normalized in alloy: one `AlloyEvent::Link`
whatever the source (a drop payload with a scheme, `link_from_drop`), so
lattice and core see one event; the cold link is a launch fact
(`lattice::Launch { restored, link }`, sticky `launchLink`).

## What it involves

In order of difficulty, hardest first:

1. DONE 2026-09-23: **desktop single instance** (`lattice/src/links.rs`).
   The first instance listens on a local endpoint in the app's own client
   dir, a Unix domain socket `link.sock` on Linux and macOS, a named pipe
   scoped by the client dir on Windows; a second instance started with a
   link of the app's scheme hands it over (one line, one acknowledgement,
   and it exits only once acknowledged, so a first instance on its way
   out cannot swallow it) and ends before any window. A stale socket file
   is told from a live one by whether anything answers. The link arrives
   as `AlloyEvent::Link`, the same path a drop takes, and the window is
   raised (`AlloyCommand::RaiseWindow`). A second instance without a link
   keeps the recorded behavior (client-storage-updates: warn and run).
2. DONE 2026-09-23: **desktop registration without an installer**, on the
   explicit call `registerProtocolHandler()` (`srt:app`, re-exported by
   core; the web's name, no arguments since the scheme is fixed). Linux:
   `~/.local/share/applications/<appId>.desktop` (Exec quoted only where
   the Desktop Entry spec requires it, since xdg-open's generic path takes
   the first word literally) and the `[Default Applications]` plus `[Added
   Associations]` entries in `~/.config/mimeapps.list`, both written
   atomically. Windows: `HKCU\Software\Classes\<scheme>` with `URL
   Protocol`, `DefaultIcon` and `shell\open\command "<exe>" "%1"` through
   the windows-registry crate. The packed binary recognizes a first
   argument of its own scheme as the launch link; everything else stays
   the app's argv. Android: the call succeeds and does nothing (the
   package declares the scheme); the dev client warns and does nothing,
   so app code registering at startup runs unchanged under the player.
3. **macOS** needs `.app` bundling in `srt pack` before it can register a
   scheme at all; the delivery half is then free (SDL already forwards
   the Apple Event). Skipped for now (2026-09-23).
4. DONE 2026-09-23: **Android.** The runner's prod manifest overlay
   declares the VIEW filter (DEFAULT and BROWSABLE) with the scheme
   spelled as the package name; the compiled manifest's pool holds that
   string once, so `patchManifest`'s package rewrite renames the scheme
   with no patcher change (verified against the built runner APK's pool).
   The scheme is the appId: RFC 8252 section 7.1 recommends reverse-DNS
   private-use schemes (`com.example.app:/path`), so every packed app
   answers to its own scheme with no configuration and cannot collide in
   practice. `SolidRTActivity` reads `getIntent().getDataString()` into
   argv next to `--restored`, and overrides `onNewIntent` for the warm
   case; the intent data is cleared before `super.onCreate` so the
   vendored `SDLActivity` stays untouched.
5. DONE 2026-09-23: **the dev-side injection**, which is also what makes
   any of this testable: `POST /__control__/link` and MCP `open_link`
   deliver a link to the running client exactly as the OS would (the
   analog of `adb shell am start -d` and `xcrun simctl openurl`), `GET`
   and `get_location` read the published location, and `srt render
   --link` starts a render at one. With it the app half: `onLink`,
   `env.launchLink`, `reportLocation` in `srt:dev`; see
   [app-routing](../plans/app-routing.md) for the consumer and the
   engine-ordering fix that made the launch facts readable at module
   scope.

## The dev client

The player registers one scheme (`solidrt:`) and dispatches, the way Expo
Go registers `exp:`: `solidrt://connect?...` from the QR code `srt run`
prints, so a phone's stock camera opens the player connected to the dev
server (no in-app scanner needed; on iOS that is the expected flow), and
`solidrt://app/<id>/...` launches an installed app with the rest as its
link. Connect links and p2p tickets become shareable strings (see
[p2p-ticket-addresses](p2p-ticket-addresses.md) on what a ticket should
then carry). A link for the *hosted* app during a dev session is
delivered through the control API, not a scheme.

## What it enables beyond links

Once a link opens a screen, the same path serves: notifications, Android
app shortcuts, Windows jump lists, `.desktop` Actions, the macOS dock
menu, Android TV home-screen channels and Watch Next (all carry links or
intents); OAuth and magic-link and payment returns on mobile (on desktop
the RFC 8252 loopback redirect over `flux:serve` needs no registration
at all); and "open with" and share-to files, which arrive through the
same SDL drop path.

## Open questions

- Universal and App Links (`https://` links verified against a domain)
  are a separate, heavier mechanism than custom schemes: a domain and a
  hosted verification file. First pass is custom schemes only.
- A link from outside is untrusted input arriving at an app that never
  asked for it, and any app can claim a custom scheme, so a link must
  never carry a secret (OAuth relies on PKCE for this). The API should
  make the untrusted nature obvious at the call site.
- Answered 2026-09-23: an unpacked dev run answers no scheme; only packed
  apps do (and the player once it registers `solidrt:`, below).

## What remains

macOS (item 3) and the dev client's own `solidrt:` scheme. The scheme
being the appId, the pack manifest needed no field; a vanity scheme would
be an additive `solidrt.scheme` later.
