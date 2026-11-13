---
title: Report what an app can use as states, not as one yes/no list
description: Flux.capabilities is a flat list of names fixed at engine start, so it cannot say "not known yet", "only a call can tell", "the user has not answered" or "it changed"; it mixes build facts, device facts and test-host overrides, and wrong answers already ship (the TV's stub picker, the Linux portal without a FileChooser, a camera with no device). Keep the list for build facts, and add a reactive three-state availability per platform facility and a permission state on the web's Permissions API shape. Starts after haptics lands.
created: 2026-11-13
---

# Report what an app can use as states, not as one yes/no list

## Symptom

`Flux.capabilities` is a `string[]` built once while the engine starts
(`flux/src/plugins/mod.rs` `build_capabilities`, extended by
`add_capability`). An app checks `includes(name)`, which gives a yes or a
no. The answer cannot be "not known yet", "only a call can tell", "the user
has not answered", or "this changed since start". Every fact that is really
one of those is forced into a boolean, so some answers are wrong today:

1. **The TV's stub picker** ([android-tv-dialog-stub](android-tv-dialog-stub.md)).
   `ACTION_OPEN_DOCUMENT` resolves to the TV's framework stub package, so
   "dialog" is listed, and every call answers with a cancel. The app shows
   an Open button that cannot work.
2. **A Linux portal without a FileChooser.** `ore::dialog::platform_available`
   checks that the portal's D-Bus *service file* is on disk, or that zenity
   is on the path. SDL's own driver choice (`SDL_Portal_detect`, SDL 3.4.18
   `src/dialog/unix/SDL_portaldialog.c`) introspects the portal object for
   `org.freedesktop.portal.FileChooser`. A session whose portal backends do
   not implement FileChooser (and has no zenity) gets "dialog" listed, and
   the call then fails with SDL's "File dialog driver unsupported". The
   link probe (`ore::link::platform_available`) has the same shape: a
   service file or `xdg-open` on the path, not an OpenURI that answers.
   ore's cheap probe is right not to block startup on D-Bus. What it lacks
   is a way to say "probably", and to correct itself.
3. **Camera, microphone and audio are listed unconditionally** under the
   gui feature (`GUI_CAPABILITIES`). The name means "flux:camera can be
   imported". An app reads it as "there is a camera". On a desktop with no
   webcam, or a server with no output device, it says yes, and the app
   learns otherwise from `createCamera`'s `error()`.
4. **A packed app that switched a capability off still lists it.**
   `"solidrt": { "capabilities": { "camera": false } }` drops
   `android.permission.CAMERA` from the APK (`packages/cli/src/pack/main.ts`),
   but the runtime does not know that. `Flux.capabilities` still lists
   "camera", and the open is denied without a prompt.
5. **"Granted" where nothing was asked.** `ore::permission::request`
   answers `true` at once off Android ("the open that follows reports what
   the OS decided"). That is not granted. It means "no prompt to raise
   ahead of use". macOS raises its own TCC prompt at the device open, so
   the honest state there is "prompt".
6. **Unknown collapsed by policy.** `ore::camera::permission_prompts`
   returns `true` before the camera driver is known ("leans toward a
   prompt may be up"). That is a correct choice for its one caller, but it
   is a third state that a bool has no room for.
7. **Nothing can change.** The list is fixed at start. A webcam plugged in,
   a portal service started after login, a permission granted in system
   settings while the app is backgrounded: none of these can reach an app
   that branched on the list.
8. **The list is not trustworthy as data.** It is a plain mutable JS array
   (`Flux.capabilities.push("camera")` works). The native copy that the dev
   client reports (`flux::capabilities()` in `flux/src/lib.rs`) repeats
   `register_capabilities` in `flux/src/alloy_plugins/mod.rs` by hand: two
   lists that must stay in sync. The test host adds "dialog" and "share" to
   the engine's list (`set_answers_only`, `set_record_only`), so the engine
   and the native report disagree there, on purpose but invisibly.

Meanwhile, three neighbors have each made their own state vocabulary,
because the shared one could not hold theirs:

- the camera and microphone sessions: `Pending | Approved | Denied`
  (`ore::camera::PermissionState`, okf/done/microphone-permission-state.md);
- haptics, in progress: `support` with `native | approximate | none` per
  intent, plus an actuator fact (okf/plans/haptics.md);
- core's environment: `SystemTheme` and `Orientation` carry `"unknown"`,
  and `env.inputDevices` is `undefined` until the runtime reports devices,
  with the seen-flags as the fallback (`packages/core/src/environment.ts`).

So the answer to "do we need more than yes/no?" is yes. But one more
value in the same list is not the fix. The list answers three different
questions with one word, and each question has its own honest states.

## Three questions, one word today

| Question | Who decides | Can it change while running? | Honest states |
|---|---|---|---|
| Is the code in this runtime? | the build (cargo features, a custom runtime's modules) | no | yes, no |
| Does this device and platform provide it? | the OS, the hardware, the session | yes (hotplug, a service starting) | available, unavailable, unknown |
| May the app use it? | the packaging (declared permissions) and the user | yes (the user answers, revokes) | granted, denied, prompt |

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
- **Test-host overrides**: dialog and share. A record-only or answers-only
  session makes them *available* in that engine. This is an availability
  fact of the session, and it fits the availability model as such.

## Proposed model

### 1. `Flux.capabilities` keeps build facts only

- It holds what the binary contains: certain from the first line of app
  code, and the same for the whole process. A boolean is the right model
  here, so the shape stays a list.
- It is frozen (`Object.freeze`), so it is data an app can trust.
- Native code has one source for it. `flux::capabilities()` and the engine
  list are built from the same function. The gui seam's list and an
  embedder's modules join through one registration path, never through a
  second hand-kept copy.
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
  emitted before the call rejects. A call that works on an `unknown`
  facility records `available`. The second call never repeats the first
  call's surprise.
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

Device facilities follow the same contract. camera and microphone are
`available` when a device is enumerated, `unavailable` when the enumeration
finished empty, and `unknown` before it finished. SDL's device added and
removed events keep the state current. haptics is the actuator fact that
the haptics plan defines.

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
- A capability that the project switched off is `"denied"` permanently.
  This needs the packed app to know its declared capabilities. On Android,
  ore can read whether the manifest declares the permission
  (`PackageManager` `requestedPermissions`), so no new packaging data is
  needed there.
- The names are the project's capability names (camera, microphone,
  haptics, later location and notifications), the same vocabulary as
  `"solidrt": { "capabilities" }`. One word per thing across the
  packaging and the runtime.

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

- Core's `capabilities` (hover, touch, keyboard nav, window size class)
  are a different thing: input affordances derived from the environment
  for the policy layer. They keep their reactive shape. The shared word
  "capabilities" across three meanings (this list, the project config, the
  core affordances) is the second open decision.
- Fidelity stays in each module (haptics `support`, `"wasm-native"`).
- A missing module still fails at import, as now.

## Sequencing

Start after haptics lands (okf/plans/haptics.md, in progress now). That
plan adds "haptics" to the list as an actuator fact and adds a `support`
query, which is exactly a device fact plus fidelity. Changing the
vocabulary under it while it is built would make both pieces of work
wrong. When it is finished, read what landed, and make its actuator fact
the first availability that moves out of the list (its `support` stays).

Then, in order, each step complete on its own:

1. One native source for the build list, frozen in JS (fixes 8).
2. `ore::Availability` and the sticky event, with dialog, link, share,
   clipboard-image and haptics moved to it. The dialog gets the
   introspection probe and the call-corrects-fact rule (fixes 1, 2, 7 for
   these facilities). Close [android-tv-dialog-stub](android-tv-dialog-stub.md)
   and [file-dialogs-no-picker-device](file-dialogs-no-picker-device.md)
   through it: the stub resolution can be `unknown` if the stub cannot be
   told apart reliably, and the first cancel-at-once then is not evidence
   enough to correct, so the stub check stays the real fix there.
3. Device availability for camera, microphone and audio output, with
   hotplug (fixes 3).
4. `navigator.permissions` over a widened `ore::permission` (camera added
   beside the microphone, the asked-before bit, state on resume), with the
   project's switched-off capabilities read as denied (fixes 4, 5, 6).

## Done looks like

- `Flux.capabilities` is frozen and lists build facts only, and the dev
  client's report comes from the same function.
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
  A packed app with `"camera": false` reads `denied` and never shows a
  prompt.
- Headless tests set availability and permission states through the test
  host and read the app's reaction.
- `packages/flux-types`, `docs/runtime/index.md` ("Capabilities, not
  platforms") and the core AGENTS topic state the three questions and the
  "show unless unavailable" rule.

## Open decisions

1. **The JS name of availability.** `Flux` is for runtime introspection,
   so `Flux.availability(name)` fits the module surface rule. A sticky
   event named for it is the reactive path. No web standard exists to
   borrow.
2. **The three meanings of "capabilities".** Keep the word for build facts
   (the runtime's) and the project config (the app's declared needs), and
   rename core's input affordances (`affordances`)? Or rename the project
   config key to `permissions`, which is what it maps to? Decide once, with
   no backwards compatibility to keep.
3. **Whether `unknown` from a probe in flight should be seen at all.** The
   alternative is that the engine waits for the fast probes before the
   first app line, so only "only a call can tell" is ever unknown. That
   costs startup time on D-Bus, which is why the proposal settles
   asynchronously. Measure the introspection on a cold session first.
