---
title: Deep links
description: "Opening the app at a link from outside: an OS registration half (scheme declaration in srt pack, the Android manifest, desktop self-registration) and an app half that is one event, onLink, plus the launch link; per-platform delivery facts, the running-instance problem on desktop, and the dev-side link injection that makes it testable without any registration."
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
[app-routing](app-routing.md), which is the consumer that turns a link
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
policy, and [app-routing](app-routing.md) is the layer that does it
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

| Platform | Registration | Already running | Today |
| --- | --- | --- | --- |
| Android | VIEW intent filter with `<data android:scheme>` | `onNewIntent` (manifest is `singleInstance`) | SDL's `onCreate` forwards only `getData().getPath()` as a drop file, so scheme and host are lost. Nothing handles `onNewIntent`. |
| macOS | `CFBundleURLTypes` in a `.app` Info.plist | Same Apple Event, sent to the running instance (LaunchServices keeps one) | SDL delivers `kAEGetURL` as `SDL_EVENT_DROP_FILE` carrying the link string. `srt pack` builds no `.app`. |
| iOS | `CFBundleURLTypes` | `openURLContexts` on the scene | SDL delivers it as a drop file with `absoluteString` (file links get the path). No iOS target yet. |
| Windows | HKCU `Software\Classes\<scheme>` with `URL Protocol` and `shell\open\command "%1"` | Starts a second process | The link already reaches the app as argv (packed binaries own their command line). No registration. |
| Linux | `.desktop` with `MimeType=x-scheme-handler/<scheme>`, `Exec ... %u`, `xdg-mime default` | Starts a second process | Same as Windows. |

Alloy maps no SDL drop events today, so no inbound path exists anywhere
yet. Platform differences are normalized in alloy: one `AlloyEvent::Link`
whatever the source (drop-file-with-a-scheme on Apple platforms, argv and
the activity on Android, argv and the single-instance socket on desktop),
so lattice and core see one event.

## What it involves

In order of difficulty, hardest first:

1. **Desktop single instance.** On Windows and Linux a link starts a new
   process. The second process must hand the link to the first and exit
   (Electron: `requestSingleInstanceLock` plus the `second-instance`
   event). A new mechanism, probably a local socket keyed by appId under
   the data root. Without it, the packed app opens twice.
2. **Desktop registration without an installer.** A packed app is a
   single executable, so nothing writes the registry key or the
   `.desktop` file. The practical model is runtime self-registration on
   an explicit call (Electron's `setAsDefaultProtocolClient`), not an
   implicit first-run write.
3. **macOS** needs `.app` bundling in `srt pack` before it can register a
   scheme at all; the delivery half is then free (SDL already forwards
   the Apple Event).
4. **Android is cheap.** The runner manifest carries a VIEW intent filter
   with a placeholder scheme string; `pack` replaces the pool string the
   way it already replaces package and label
   (`packages/cli/src/pack/android/apk.ts`, `patchManifest`). Default the
   scheme to the appId: RFC 8252 section 7.1 recommends reverse-DNS
   private-use schemes (`com.example.app:/path`), so every packed app
   answers to its own scheme with no configuration and cannot collide in
   practice. `SolidRTActivity` reads `getIntent().getDataString()` into
   argv next to `--restored`, and overrides `onNewIntent` for the warm
   case. It clears the intent data before `super.onCreate` so the vendored
   `SDLActivity` stays untouched and its lossy drop-file path does not
   fire.
5. **The dev-side injection**, which is also what makes any of this
   testable: `POST /__control__/link` and MCP `open_link` deliver a link
   to the running client exactly as the OS would (the analog of `adb
   shell am start -d` and `xcrun simctl openurl`), and `srt render
   --link` starts a render at one. None of it needs OS registration, so
   it is the first thing to build.

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
- Whether an unpacked dev run answers a scheme at all, or only packaged
  apps and the player.

## Why deferred

No product need yet, and the packaging half cannot be done well until the
pack manifest grows a place to declare it. The dev-side injection (item 5)
has no such dependency and serves the routing item on its own.
