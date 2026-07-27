---
type: backlog-item
title: Deep links
description: "Opening the app at a URL from outside: an OS registration half (scheme declaration in srt pack and the Android manifest) and an app half that is just onOpenUrl."
status: deferred
timestamp: 2026-07-26T00:00:00Z
---

# Deep links

An external source hands the app a URL and the app opens at the place that
URL names: a link in a browser, a QR code, another app, a notification.
Expo's `expo-linking` is the closest reference for what app developers
expect to be there.

Raised 2026-07-26 while writing the website's Core concepts, where the
absence would otherwise have to be explained. Deliberately NOT flagged on
the website (no "maybe we will implement this" notes there); this file is
the record.

## It splits in two, and only one half is hard

**The OS half is registration**, per platform, and it is packaging work
rather than runtime work: an intent filter in the Android manifest, a
`MimeType`/`x-scheme-handler` entry in the Linux `.desktop` file, a
registry protocol handler on Windows, `CFBundleURLTypes` on macOS. That
means `srt pack` (and the manifest it writes) grows a field for the scheme
the app answers to, and the Android manifest stops being fully static.

**The app half is one event carrying a URL.** No router, no history, no
location object: `onOpenUrl((url) => ...)` next to the existing `onBack` in
core's window module, and the app decides what that means for its own
screen state. This is the part worth saying out loud in any design
discussion, because "URL support" reads to a web developer as an implied
routing system, and nothing here requires one.

Cold start needs care: the launch URL exists before the app's first frame,
so it has to be readable by a listener registered during the first render
rather than fired and lost. The sticky-event mechanism behind
`env.visibility` (forge::events StickyCache) is the existing precedent.

## In-repo precedent

The launcher already treats an externally supplied string as an entry
point: QR scan to a dev-server address, and `srt:apps` launching an app by
id. A launcher that is also the URL dispatcher (scheme -> installed app id
-> launch with the URL) is the natural shape on Android, where only one
process can own an intent filter, and it is worth considering before
per-app registration is designed.

## Open questions

- Universal/app links (plain `https://` URLs verified against a domain) are
  a separate, heavier mechanism than custom schemes. First pass should be
  custom schemes only.
- A URL from outside is untrusted input arriving at an app that never asked
  for it. Whatever API lands should make that obvious at the call site.
- Whether the dev client answers a scheme at all, or only packaged apps.

## Why deferred

No product need yet, and the packaging half cannot be done well until the
pack manifest grows a place to declare it.