---
title: Report what an app can use as states, not as one yes/no list
description: Flux.capabilities is a flat list of names fixed for the process, so it cannot say "not known yet", "only a call can tell", "the user has not answered" or "it changed"; it mixes build facts, device facts and test-host overrides, and wrong answers already ship (the TV's stub picker, the Linux portal without a FileChooser, a camera with no device). Keep the list for build facts (the modules compiled in, minus what the project switched off), and add a reactive three-state availability per platform facility and a permission state on the web's Permissions API shape.
created: 2026-11-13
---

# Report what an app can use as states, not as one yes/no list

## Symptom

`Flux.capabilities` is a `string[]` built once for the process: the base
list while the engine starts (`flux/src/plugins/mod.rs` `build_capabilities`),
the gui names as a plugin (`register_capabilities` in
`flux/src/alloy_plugins/mod.rs`, through `add_capability`), and the
platform facts probed once (the dialog when the window opens,
`ore/src/window/open.rs`; link, share and haptics on their first read). An
app checks `includes(name)`, which gives a yes or a no. The answer cannot
be "not known yet", "only a call can tell", "the user has not answered",
or "this changed since start". Every fact that is really one of those is
forced into a boolean, so some answers are wrong today:

1. **The TV's stub picker** ([android-tv-dialog-stub](android-tv-dialog-stub.md)).
   `ACTION_OPEN_DOCUMENT` resolves to the TV's framework stub package, so
   "dialog" is listed, and every call answers with a cancel. The app shows
   an Open button that cannot work.
2. **A Linux portal without a FileChooser.** `ore::dialog::platform_available`
   checks that there is a session bus and the portal's D-Bus *service file*
   is on disk, or that zenity is on the path. SDL's own driver choice
   (`SDL_Portal_detect`, SDL 3.4.18 `src/dialog/unix/SDL_portaldialog.c`)
   introspects the portal object for `org.freedesktop.portal.FileChooser`.
   A session whose portal backends do not implement FileChooser (and has
   no zenity) gets "dialog" listed, and the call then fails with SDL's
   "File dialog driver unsupported". The link probe
   (`ore::link::platform_available`) has the same shape: a service file or
   `xdg-open` on the path, not an OpenURI that answers. ore's cheap probe
   is right not to block startup on D-Bus. What it lacks is a way to say
   "probably", and to correct itself.
3. **Camera, microphone and audio are listed unconditionally** under the
   gui feature (`GUI_CAPABILITIES`). The name means "flux:camera can be
   imported". An app reads it as "there is a camera". On a desktop with no
   webcam, or a server with no output device, it says yes, and the app
   learns otherwise from `createCamera`'s `error()`.
4. **A packed app that switched a capability off still lists it.**
   `"solidrt": { "capabilities": { "camera": false } }` drops
   `android.permission.CAMERA` from the APK (`packages/cli/src/pack/main.ts`,
   `ANDROID_CAPABILITY_PERMISSIONS`), but the runtime does not know that.
   `Flux.capabilities` still lists "camera", and the open is denied without
   a prompt. Haptics handles the same case on its own ([haptics](../done/haptics.md),
   "Without the capability"): the `Vibrator` intents report `none`, the
   calls are no-ops, and one warning names the capability. So one fact,
   "the project left it out", already has two readings, and the camera has
   none. The default-on camera capability also makes Play infer a required
   camera (okf/tiny.md, "packed Android apps"), the packaging side of the
   same fact.
5. **"Granted" where nothing was asked.** `ore::permission::request`
   answers `true` at once off Android ("the open that follows reports what
   the OS decided"). That is not granted. It means "no prompt to raise
   ahead of use". macOS raises its own TCC prompt at the device open, so
   the honest state there is "prompt".
6. **Unknown collapsed by policy.** `ore::camera::permission_prompts`
   returns `true` before the camera driver is known ("leans toward a
   prompt may be up"). That is a correct choice for its one caller, but it
   is a third state that a bool has no room for.
7. **Nothing can change.** The list is fixed for the process. A webcam
   plugged in, a portal service started after login, a permission granted
   in system settings while the app is backgrounded: none of these can
   reach an app that branched on the list. The one precedent for a fact
   that moves is the sticky `inputDevices` event, and even that misses a
   touchscreen attached after start (okf/tiny.md, "Android touch device
   list").
8. **The list is not trustworthy as data.** It is a plain mutable JS array
   (`Flux.capabilities.push("camera")` works). The native copy that the dev
   client reports (`flux::capabilities()` in `flux/src/lib.rs`) repeats
   `register_capabilities` by hand, and `lattice/src/go/connection.rs`
   chains a third list, lattice's own `runtime:*` module names
   (`lattice/src/entry.rs`): three lists that must stay in sync. The test
   host adds every platform name to the engine's list when the platform
   lacks it: dialog through `set_answers_only`, and link, share, haptics
   and clipboard-image through their `set_record_only`. So the engine and
   the native report disagree there, on purpose but invisibly.

Meanwhile, three neighbors have each made their own state vocabulary,
because the shared one could not hold theirs:

- the camera and microphone sessions: `Pending | Approved | Denied`
  (`ore::camera::PermissionState`, [microphone-permission-state](../done/microphone-permission-state.md));
- haptics, landed: `Support` with an `actuator` fact and a `Level` of
  `native | approximate | none` per intent (`ore/src/haptics.rs`,
  [haptics](../done/haptics.md));
- core's environment: `SystemTheme` and `Orientation` carry `"unknown"`,
  and `env.inputDevices` is `undefined` until the runtime reports devices,
  with the seen-flags as the fallback (`packages/core/src/environment.ts`).

So the answer to "do we need more than yes/no?" is yes. But one more
value in the same list is not the fix. The list answers three different
questions with one word, and each question has its own honest states.

## Three questions, one word today

| Question | Who decides | Can it change while running? | Honest states |
|---|---|---|---|
| Is the code in this runtime? | the build (cargo features, a custom runtime's modules) and the packaging (the project's switched-off capabilities) | no | yes, no |
| Does this device and platform provide it? | the OS, the hardware, the session | yes (hotplug, a service starting) | available, unavailable, unknown |
| May the app use it? | the user | yes (the user answers, revokes) | granted, denied, prompt |

A fourth axis, **fidelity** (haptics `native | approximate`, the wasm lane,
compressed textures), is facility-specific. It stays in each module's own
support query, as haptics does. A shared vocabulary for "how well" would
be either too coarse to use or too wide to stay honest.

Where today's names sit:

- **Build facts**: sqlite, fs, http, tls, p2p, p2p-relay, process, path,
  subprocess, svg, image, wasm, wasm-native, isolate, tty, ktx2, test,
  camera, microphone, audio, gpu, spatial, font, video, video-encode, and a
  custom runtime's `runtime:*` modules (`lattice/src/entry.rs`).
- **Device or platform facts**: link, dialog (probed), share,
  clipboard-image (fixed per target today, but a fact of the platform, and
  share will be probed when a desktop backend comes), haptics (an
  actuator), and the device half of camera, microphone and audio.
- **Permission facts**: camera, microphone, haptics' `VIBRATE`, later
  location, notifications and exact alarms (okf/design/platform-layering.md,
  "Permissions").
- **Test-host overrides**: dialog, link, share, haptics and clipboard-image,
  every platform name. A record-only or answers-only session makes them
  *available* in that engine. This is an availability fact of the session,
  and it fits the availability model as such.

## Proposed model

### 1. `Flux.capabilities` keeps build facts only

- It holds what this program contains: certain from the first line of app
  code, and the same for the whole process. A boolean is the right model
  here, so the shape stays a list.
- It is frozen (`Object.freeze`), so it is data an app can trust. The
  freeze runs after the whole plugin chain, since the gui seam, an
  embedder's modules and the test host's overrides all add names as
  plugins today.
- Native code has one source for it. `flux::capabilities()`, the engine
  list and the dev client's report are built from the same function. The
  gui seam's list and an embedder's modules join through one registration
  path, never through a second hand-kept copy.
- **A capability the project switched off is absent from the list**, like
  a module not compiled in. "The developer left it out" is a packaging
  fact, fixed for the process and the same on every platform, so it is a
  build fact, not a permission state. This needs the packed program to
  carry its resolved capability set (`resolveCapabilities` in
  `packages/cli/src/lib/project.ts`) to the runtime; the Android manifest's
  `requestedPermissions` is a cross-check there, not the source, since
  desktop has no manifest. The module import still works and its calls
  reject with `NotSupportedError`, as haptics already warns once and plays
  nothing (fixes 4 in one place for every capability).
- The platform names (link, dialog, share, clipboard-image, haptics) leave
  it and move to availability (2). camera, microphone and audio stay as
  build names ("the module is here"), and their device half moves to (2).

### 2. Availability: three states, reactive, owned by ore

Each platform facility reports one of:

- `"available"`: the platform says yes, and a call is expected to work.
- `"unavailable"`: a call cannot work here. It rejects with
  `NotSupportedError`. An app hides the feature.
- `"unknown"`: the runtime cannot tell yet. It covers two cases, and each
  facility documents which one applies:
  - *not settled*: a probe is in flight (the D-Bus introspection, the
    camera enumeration that pipewire does asynchronously, the window's
    dialog probe). The state moves to available or unavailable when the
    probe ends. It never stays unknown after its probe.
  - *only a call can tell*: no probe reaches the answer without doing the
    work (an intent resolving to a stub, an opener whose handler may be
    missing). The state stays unknown until a call settles it.

Two rules make this exact, not advisory:

- **A call corrects the fact.** When a call fails because the facility is
  absent (SDL's "driver unsupported", ore's `DialogError::Unavailable`,
  `LinkError::Unavailable`), ore records `unavailable` and the change is
  emitted before the call rejects, whatever the state was. A call that
  works on an `unknown` facility records `available`. The second call
  never repeats the first call's surprise.
- **Show unless unavailable.** The documented app rule is "offer the
  feature unless it is unavailable, and handle `NotSupportedError`". So
  `unknown` never hides a feature that would have worked.

Placement, by okf/design/platform-layering.md:

- ore owns the facts. Each facility has `availability() -> Availability`
  (one shared `enum Availability { Available, Unavailable, Unknown }` in
  ore) and a change notification. A probe's cost and timing are decided in
  the facility. For example, the Linux dialog keeps today's file check as
  the fast first answer (unavailable when neither the service file nor
  zenity is there, unknown otherwise), runs the FileChooser introspection
  off the main thread, and settles from its result.
- flux marshals it onto the event bus as one sticky event (the mechanism
  in `flux/src/plugins/events.rs`, as `inputDevices` and `systemTheme`
  are delivered), with a synchronous read for code that is not reactive.
  The JS name is the first open decision below.
- core gives it a reactive surface beside `env`, so a Solid component that
  reads it updates when a probe settles or a device arrives.
- The test host sets it like any other session fact. An answers-only
  dialog session is `available` in that engine, and the native dev-client
  report states the client's own facts, so the two may differ, as they do
  today, but now by design and visibly.
- The dev client reports the states beside the list. The console's server
  panel (`apps/console/src/servers.ts`) and the MCP `list_clients` tool
  read `info.capabilities` today as "the capability names compiled into
  that runtime"; once the platform names leave the list, an agent must be
  able to see "dialog unknown" or "camera denied" on a client without
  writing an app for it.

Device facilities follow the same contract. camera and microphone are
`available` when a device is enumerated, `unavailable` when the enumeration
finished empty, and `unknown` before it finished. SDL's device added and
removed events keep the state current, the same way the sticky
`inputDevices` event is meant to (and the touchscreen it misses, okf/tiny.md,
is fixed by the same rule: a device seen is a device present). haptics is
the actuator fact that landed, `ore::haptics::available()`.

### 3. Permission: the web's Permissions API, simplified

The web already has this question and its answer. `navigator.permissions.query({ name })`
resolves to a `PermissionStatus` whose `state` is `"granted" | "denied" | "prompt"`
and which fires `change`. Keep the name and the shape (CLAUDE.md, API
design), and simplify the semantics:

- The status is live: `state` updates, and `change` fires when the user
  answers a prompt, or returns from system settings with a new answer.
- `"prompt"` is the honest state wherever the OS asks at the open (macOS
  TCC, Android before the first request). ore's `request` stops answering
  `true` for "nothing to ask ahead of use". It answers with the state, and
  the open still reports the OS's decision.
- A capability the project switched off never reaches this layer: it is
  absent from the build list (1), so a query for it answers as for an
  unknown name. The permission state is the user's answer only.
- The names are the project's capability names (camera, microphone,
  haptics, later location and notifications), the same vocabulary as the
  project config. One word per thing across the packaging and the runtime.
- The per-open session state stays. `Pending | Approved | Denied` on a
  camera or microphone session is the progress of one open (the OS prompt
  is up, the device streams, the device was refused), and
  `createCamera`'s `error()` keeps reporting it. The standing permission
  is a different fact: `prompt` is what a session reads before its open
  goes `Pending`, and a `Denied` open settles the permission to `denied`.
  Neither replaces the other.

Engine-level traps that the implementation must face, not skip:

- **Android cannot tell "never asked" from "denied for good".**
  `checkSelfPermission` is denied in both cases, and
  `shouldShowRequestPermissionRationale` is false in both. The only exact
  answer is to remember that we asked: ore stores the asked-before bit per
  permission in the app's own storage, and clears it when the permission
  becomes granted. Without it, `"prompt"` and `"denied"` are guesses.
- **"Only this time" grants** (Android 11+) end when the app leaves the
  foreground. The state is read again on resume (ore's lifecycle watch),
  not cached from the grant.
- **Desktop states are per platform.** macOS has an exact answer
  (`AVCaptureDevice.authorizationStatus`: not determined, authorized,
  denied, restricted). Windows has a per-device privacy switch, and how
  far an unpackaged app can read it must be checked. Linux outside a
  sandbox has no permission model, so `"granted"` is correct there; inside
  a sandbox the camera portal decides. Each platform's mapping is written
  down in ore, one contract, as `ore::permission` already states it.

### What stays as it is

- Core's `capabilities` (hover, touch, keyboard nav, window size class,
  `packages/core/src/capabilities.ts`) are a different thing: input
  affordances derived from the environment for the policy layer. They
  keep their reactive shape and their name (see the second open decision).
- Fidelity stays in each module (haptics `support`, `"wasm-native"`).
- A missing module still fails at import, as now.

## Neighbors that build on this

Three open items reach for the list or the permission question, and
should adopt the three states from the start rather than add a boolean
each:

- [location-module](location-module.md) promises "availability by name
  via Flux.capabilities" and permission as an `error()` state. Its build
  fact is the module, its availability is GeoClue2 or the platform
  service, and its permission is a `navigator.permissions` name.
- [runtime-policy-registry](runtime-policy-registry.md) borrows the
  "`Flux.capabilities`-style ask by name" shape for the read side of
  policies. The build list stays a list; the reactive availability
  surface is the shape a policy's current value wants.
- okf/tiny.md, "Android touch device list" and "packed Android apps", are
  the hotplug case and the switched-off-capability case of this note,
  seen once each.

## Sequencing

Haptics landed first ([haptics](../done/haptics.md)): its actuator fact
(`ore::haptics::available()`) is a device fact and its `support` query is
fidelity, which is exactly the split this note draws. Its actuator fact is
the first availability that moves out of the list; `support` stays.

Then, in order, each step complete on its own:

1. One native source for the build list, frozen in JS after the plugin
   chain, and the project's switched-off capabilities absent from it
   (fixes 4, 8). The five test-host overrides keep working through this
   step: they are plugins, so they run before the freeze, and they leave
   the list in step 2.
2. `ore::Availability` and the sticky event, with dialog, link, share,
   clipboard-image and haptics moved to it, and the test-host overrides
   with them. The dialog gets the introspection probe and the
   call-corrects-fact rule (fixes 1, 2, 7 for these facilities). Close
   [android-tv-dialog-stub](android-tv-dialog-stub.md) and
   [file-dialogs-no-picker-device](file-dialogs-no-picker-device.md)
   through it: the stub resolution can be `unknown` if the stub cannot be
   told apart reliably, and the first cancel-at-once then is not evidence
   enough to correct, so the stub check stays the real fix there. The dev
   client's info message carries the states from this step on.
3. Device availability for camera, microphone and audio output, with
   hotplug (fixes 3, and the touchscreen line in okf/tiny.md by the same
   rule).
4. `navigator.permissions` over a widened `ore::permission` (camera added
   beside the microphone, the asked-before bit, state on resume), with
   the session state mapped as above (fixes 5, 6).

## Done looks like

- `Flux.capabilities` is frozen and lists build facts only, a switched-off
  capability is absent from it on every platform, and the dev client's
  report comes from the same function.
- On the TV, the dialog is `unavailable` (or `unknown`, settled by its
  first call), never `available` with a cancel. On a Linux session with
  no FileChooser backend and no zenity, the dialog reads `unknown` at
  start, settles to `unavailable` without a call, and a call rejects with
  `NotSupportedError`.
- On a desktop with no webcam, the camera reads `unavailable`. Plugging
  one in moves it to `available` without a restart, and a reactive
  component that reads it updates.
- On Android, the camera's permission reads `prompt` before the first
  request, `granted` or `denied` after the answer, `denied` after a "don't
  ask again", and `prompt` again after an "only this time" grant expires.
  A packed app with `"camera": false` has no "camera" in its list and
  never shows a prompt.
- Headless tests set availability and permission states through the test
  host and read the app's reaction.
- `list_clients` and the console's server panel show a client's
  availability and permission states beside its build list.
- `packages/flux-types` (`capabilities` as `readonly string[]`),
  `docs/runtime/index.md` ("Capabilities, not platforms") and the core
  AGENTS topic state the three questions and the "show unless
  unavailable" rule.

## Open decisions

1. **The JS name of availability.** `Flux` is for runtime introspection,
   so `Flux.availability(name)` fits the module surface rule. A sticky
   event named for it is the reactive path. No web standard exists to
   borrow.
2. **The three meanings of "capabilities".** Recommendation: rename the
   project config key to `permissions`. The packer maps it one-to-one
   onto declared permissions today (`ANDROID_CAPABILITY_PERMISSIONS`), and
   the `vibration` to `haptics` rename showed there is no compatibility to
   keep. `Flux.capabilities` keeps the word for build facts, and core's
   input affordances keep theirs, since that name ships in every
   scaffold. Decide once, before step 1 carries the resolved set to the
   runtime under one name or the other.
3. **Whether `unknown` from a probe in flight should be seen at all.** The
   alternative is that the engine waits for the fast probes before the
   first app line, so only "only a call can tell" is ever unknown. That
   costs startup time on D-Bus, which is why the proposal settles
   asynchronously. Measure the introspection on a cold session first.
