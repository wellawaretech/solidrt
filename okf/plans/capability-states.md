---
title: Report what an app can use as states, not as one yes/no list
description: Flux.capabilities is a flat list of names fixed for the process, so it cannot say "not known yet", "only a call can tell", "the user has not answered" or "it changed"; it mixes build facts, device facts and test-host overrides, and wrong answers already ship (the TV's stub picker, the Linux portal without a FileChooser, a camera with no device). Keep the list for build facts (the modules compiled in, minus what the project declared it does not use), add a reactive three-state availability per platform facility and a permission state on the web's Permissions API shape, and name the packaging declaration as the fourth, platform-neutral axis it is.
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

1. **The TV's stub picker** ([android-tv-dialog-stub](../backlog/android-tv-dialog-stub.md)).
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
questions with one word, the project config answers a fourth with the
same word, and each question has its own honest states.

## Four questions, one word today

| Question | Who decides | Can it change while running? | Honest states |
|---|---|---|---|
| Is the code in this runtime? | the build (cargo features, a custom runtime's modules) and the packaging (the project's switched-off capabilities) | no | yes, no |
| Does this device and platform provide it? | the OS, the hardware, the session | yes (hotplug, a service starting) | available, unavailable, unknown |
| May the app use it? | the user | yes (the user answers, revokes) | granted, denied, prompt |
| Does the app declare that it uses it? | the developer, at pack time | no | declared, not declared |

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
- **Permission facts**: camera, microphone, later geolocation,
  notifications and exact alarms (okf/design/platform-layering.md,
  "Permissions"). Not haptics and not internet: Android's `VIBRATE` and
  `INTERNET` are install-time declarations, and no other platform asks
  the user for either. They are declaration facts (below).
- **Declaration facts**: what the project says the app uses, in the
  config key `capabilities` today (camera, microphone, haptics,
  internet). Fixed at pack time, mapped by each packer to its platform's
  own declaration; today only the Android packer exists, so the map looks
  like a list of Android permissions. It is not one (see "4. The
  declaration").
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
- **A name the project declared it does not use is absent from the
  list**, like a module not compiled in. "The developer left it out" is a
  packaging fact, fixed for the process and the same on every platform,
  so it is a build fact, not a permission state. This needs the packed
  program to carry its declared set (section 4) to the runtime in the
  manifest; the Android manifest's `requestedPermissions` is a cross-check
  there, not the source, since desktop has no manifest. The module import
  still works and its calls reject with `NotSupportedError`, as haptics
  already warns once and plays nothing (fixes 4 in one place for every
  name). This runtime half is what gives the declaration one meaning on
  every platform: without it, a switched-off camera is refused on Android
  (no permission declared), kills the app on macOS (no usage description)
  and simply works on bare Linux.
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
- A name the project declared it does not use never reaches this layer:
  it is absent from the build list (1), so a query for it answers as for
  an unknown name. The permission state is the user's answer only.
- The names are the web's permission names (`camera`, `microphone`, later
  `geolocation`, `notifications`), and the declaration (4) uses the same
  words, so one word names one thing across the packaging and the
  runtime. Haptics and internet are not permission names: no platform
  asks the user for them. Where a platform has no prompt for a name
  (Linux outside a sandbox, haptics anywhere), the honest state is
  `granted`, as the web answers for anything it does not gate.
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

### 4. The declaration: what the app uses, mapped per packer

The project config key says what the app uses (`camera`, `microphone`,
`haptics`, `internet`; later `geolocation`, `notifications`). It is a
pack-time fact, fixed for the process, the same on every platform, and
every packer maps it to its platform's own declaration. Only the Android
packer exists today, which is why the map in `packages/cli/src/pack/main.ts`
(`ANDROID_CAPABILITY_PERMISSIONS`) reads like a one-to-one list of
Android permissions. It is one platform's column of this table:

| Name | Build fact | Availability | Permission | Android | iOS and macOS | Windows (MSIX) | Flatpak | Snap |
|---|---|---|---|---|---|---|---|---|
| `camera` | `camera` (module) | device | `camera` | `CAMERA`; `uses-feature android.hardware.camera required=false` | `NSCameraUsageDescription`; sandbox `com.apple.security.device.camera` | `webcam` | camera portal | `camera` plug |
| `microphone` | `microphone` (module) | device | `microphone` | `RECORD_AUDIO` | `NSMicrophoneUsageDescription`; sandbox `com.apple.security.device.audio-input` | `microphone` | `--socket=pulseaudio` | `audio-record` plug |
| `haptics` | `haptics` (module) | actuator | none | `VIBRATE` | nothing | nothing | nothing | nothing |
| `internet` | none (`http` is the build fact) | none | none | `INTERNET` | sandbox `com.apple.security.network.client`; iOS nothing | `internetClient` | `--share=network` | `network` plug |
| `geolocation` (later) | the location module | service | `geolocation` | `ACCESS_FINE_LOCATION` | `NSLocationWhenInUseUsageDescription` | `location` | geoclue portal | `location-observe` plug |
| `notifications` (later) | the notifications module | none | `notifications` | `POST_NOTIFICATIONS` | nothing declared | nothing | notification portal | nothing |

Names with no declaration anywhere (`dialog`, `link`, `share`,
`clipboard-image`, `audio` output) are not in the key: they are build or
availability facts only. Several cells read "nothing", which is the
point: on those platforms the declaration has no effect of its own, and
the runtime half in (1) is what makes `"camera": false` mean the same
everywhere.

The key keeps a platform-neutral name. `permissions` is Android's and
Flatpak's word for it; Apple says usage description and entitlement,
Windows says capability, Snap says plug. The word in use, `capabilities`,
is the MSIX word and collides with `Flux.capabilities` and core's
`capabilities`, so it is renamed `uses`: `"solidrt": { "uses": { "camera": false } }`,
which reads as the sentence it is and names no platform. The packer's
map is per target, as the table's columns are, and the manifest carries
the resolved set under `uses`.

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

- [location-module](../backlog/location-module.md) promises "availability by name
  via Flux.capabilities" and permission as an `error()` state. Its build
  fact is the module, its availability is GeoClue2 or the platform
  service, and its permission is a `navigator.permissions` name.
- [runtime-policy-registry](../backlog/runtime-policy-registry.md) borrows the
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
   [android-tv-dialog-stub](../backlog/android-tv-dialog-stub.md) and
   [file-dialogs-no-picker-device](../backlog/file-dialogs-no-picker-device.md)
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
  A packed app with `"uses": { "camera": false }` has no "camera" in its
  list and never shows a prompt, on every platform.
- Headless tests set availability and permission states through the test
  host and read the app's reaction.
- `list_clients` and the console's server panel show a client's
  availability and permission states beside its build list.
- `packages/flux-types` (`capabilities` as `readonly string[]`),
  `docs/runtime/index.md` ("Capabilities, not platforms") and the core
  AGENTS topic state the four questions and the "show unless
  unavailable" rule, and `packages/cli/src/pack/docs.md` carries the
  declaration table.

## Open decisions

1. **The JS name of availability.** `Flux` is for runtime introspection,
   so `Flux.availability(name)` fits the module surface rule. A sticky
   event named for it is the reactive path. No web standard exists to
   borrow.
2. **The three meanings of "capabilities".** Decided (2026-11-15): the
   project config key becomes `uses` (section 4), a neutral word for the
   declaration; `permissions` was rejected as one platform's word for a
   cross-platform fact. `Flux.capabilities` keeps the word for build
   facts, and core's input affordances keep theirs, since that name ships
   in every scaffold. The `vibration` to `haptics` rename showed there is
   no compatibility to keep.
3. **Whether `unknown` from a probe in flight should be seen at all.** The
   alternative is that the engine waits for the fast probes before the
   first app line, so only "only a call can tell" is ever unknown. That
   costs startup time on D-Bus, which is why the proposal settles
   asynchronously. Measure the introspection on a cold session first.

## Plan (2026-11-15)

Decisions taken for the plan (the open decisions above, resolved):

1. The JS read of availability is `Flux.availability(name)` (sync, every
   build), the reactive path is one sticky event `availability` whose
   payload is the whole map (`{ dialog: "unknown", link: "available", ... }`),
   and core exposes `availability(name)` as a reactive read beside `env`.
2. The project config key `capabilities` is renamed `uses` (section 4);
   the manifest carries the declared set under the same name.
   `Flux.capabilities` keeps the word for build facts; core's input
   affordances keep theirs.
3. `unknown` from a probe in flight is visible: the fast probes answer at
   start, the slow ones settle asynchronously. No startup wait on D-Bus.
4. Haptics and internet are declarations, not permission names;
   `navigator.permissions` knows the web's names only.
5. All four steps are in scope, in order, each reviewed before the next
   is started.

Each step is complete on its own and is reviewed before the next starts.
Every step ends with `cargo test -p flux --lib --features gui`,
`cargo test -p ore`, `bun run sol check` and `bun run sol test` green, the
`.d.ts` in `packages/flux-types` matching the surface, and
`bun scripts/build-okf-index.ts`.

### Step 1: one native source for the build list, frozen, minus what the project declared it does not use

flux (`flux/src/plugins/mod.rs`, `flux/src/lib.rs`):

- A per-engine `CapabilityRegistry` userdata (listed names in order,
  excluded names), stored at context init and seeded from
  `base_capabilities()`. `add_capability(ctx, name)` pushes into it (no
  duplicate) instead of appending to the JS array. New
  `exclude_capability(ctx, name)` marks a name excluded; new
  `capability_excluded(ctx, name) -> bool` for a plugin that must refuse a
  call (camera, microphone open).
- `Flux.capabilities` is defined once, after the whole plugin chain
  (after `for setup in setups` in the context build), as a frozen array
  (`Object.freeze`) on a non-writable, non-configurable property, holding
  listed minus excluded. `Flux` itself is frozen there too.
- `flux::capabilities()` (native, for the dev client's report) becomes
  `base_capabilities()` plus `GUI_CAPABILITIES` under the gui feature, and
  nothing hand-listed: the dialog and clipboard probes it repeats today go
  in step 2. The report is the build's list; the engine's list is the
  build's list minus the app's exclusions (a dev client runs many apps on
  one connection, so the app's exclusions are not a client fact).
- `flux:camera` and `flux:microphone` reject their open with
  `NotSupportedError` ("the project switched the camera off") when
  `capability_excluded`. Haptics already answers `none` without `VIBRATE`
  on Android; on every platform `feedbackSupport` reports `Support::none()`
  and `feedback` is a no-op with one warning when excluded (the one place
  the fact is read, in the flux plugin, so ore stays unaware of the
  project).

CLI (`packages/cli/src/lib/project.ts`, `pack/main.ts`, `pack/android/apk.ts`,
`lib/project.ts` `buildManifest`, `pack/layout.ts`, `init/scaffold/`):

- The config key `capabilities` becomes `uses` (`USES`, `Use`,
  `resolveUses`). A config with `capabilities` fails with "rename the key
  to uses", like the `vibration` to `haptics` rename.
  `ANDROID_CAPABILITY_PERMISSIONS` becomes `ANDROID_DECLARATIONS`, the
  Android column of the table in section 4; the table itself goes in
  `pack/docs.md`. The scaffold's `package.json` and `AGENTS.md`, and
  `docs/` follow.
- The dev manifest and the pack manifest carry `"uses": [...]`, the
  declared set, when a project exists. No key means everything is used
  (older manifests, a file served on its own).

lattice (`lattice/src/manifest.rs`, `lib.rs`, `payload.rs`, `go/store.rs`):

- `Manifest.uses: Option<Vec<String>>` and
  `Manifest::excluded_capabilities() -> Vec<&'static str>`: of the names
  that are both a declaration and a build capability (`camera`,
  `microphone`, `haptics`; `internet` has no build name and stays a
  packaging fact), the ones absent when the key is present.
- The engine build applies them as a plugin
  (`flux::exclude_capability`), for the installed version's manifest in
  the go client, the packed payload's and the `--assets` folder's alike,
  through one accessor beside `app_fonts`.

Types and docs: `packages/flux-types/index.d.ts` `capabilities: readonly
string[]`; `docs/runtime/index.md` ("Capabilities, not platforms") says
the list is frozen and holds build facts; `packages/cli/src/pack/docs.md`.

Tests: a flux `.test.ts` that `Flux.capabilities` is frozen and
`Object.isFrozen(Flux)`; a lattice unit test for
`excluded_capabilities`; a CLI test for the `uses` key and the manifest
field; the `vibration`/`capabilities` rename failures.

### Step 2: `ore::Availability`, the sticky event, five facilities moved

ore (`ore/src/availability.rs`, new, and each facility):

- `pub enum Availability { Available, Unavailable, Unknown }` and
  `pub enum Facility { Link, Dialog, Share, ClipboardImage, Haptics, Camera, Microphone, AudioOutput }`
  with `name()` (`link`, `dialog`, `share`, `clipboard-image`, `haptics`,
  `camera`, `microphone`, `audio`). One table of atomics,
  `availability::of(Facility) -> Availability`, `pub(crate) set(Facility,
  Availability)` which records and, on a change, queues
  `PlatformEvent::Availability { facility, state }` and wakes the pump
  (the watch poll drains the queue, as it does for the keyboard facts).
  Camera, microphone and audio stay `Unknown` until step 3.
- Dialog: `available() -> bool` becomes `availability()`. Windows and
  macOS: Available. iOS, a hidden or offscreen surface: Unavailable.
  Android: the activity's `dialogsAvailable` resolves the intent and
  treats a resolution to the TV framework's stub package
  (`com.google.android.tv.frameworkpackagestubs`) as no picker:
  Available or Unavailable, never a cancel-at-once. Linux: Unavailable
  when there is neither the portal service file on a session bus nor
  zenity; Available when zenity is on the path; Unknown with the portal
  alone, then the FileChooser introspection (step 2b) settles it. The
  session's `begin` refuses only on Unavailable; `Unknown` shows.
- Link: the same shape. Linux: Unavailable with neither portal nor
  `xdg-open`; Available with `xdg-open`; Unknown with the portal alone,
  settled by the OpenURI introspection (step 2b). Elsewhere Available.
- Share: Android Available, elsewhere Unavailable. Clipboard image: the
  desktops Available, elsewhere Unavailable. Haptics: the actuator fact,
  probed once.
- A call corrects the fact, in the session before it answers: an
  `Unavailable` error from the platform (SDL's "driver unsupported",
  `DialogError::Unavailable`, `LinkError::Unavailable`) sets Unavailable;
  a platform answer that worked (a cancel included) on Unknown sets
  Available.
- `DialogStatus.available: bool` becomes `availability: Availability`.
- 2b, Linux only, after 2a lands: the portal introspection
  (`org.freedesktop.DBus.Introspectable.Introspect` on
  `/org/freedesktop/portal/desktop`), off the main thread, once, settling
  Dialog on `org.freedesktop.portal.FileChooser` and Link on
  `org.freedesktop.portal.OpenURI`. Needs a D-Bus client (see the
  dependency decision below); without one, the portal-alone case stays
  `unknown` until a call settles it, which is still correct.

alloy: `AlloyEvent::Availability { facility, state }` translated from the
platform event, nothing else (a fact passed through).

flux:

- `flux/src/ore_plugins/availability.rs` (every build): a per-engine
  `Overrides` map (facility name to Availability) and the snapshot
  (`ore::availability::of` for each facility, overrides applied).
  `Flux.availability(name)` returns `"available" | "unavailable" | "unknown"`
  and throws a `TypeError` on an unknown name. `set_override(ctx, name,
  state)`, called by `set_answers_only` and the four `set_record_only`
  (Available in that engine), which no longer touch the capability list.
  `emit(ctx)` emits the sticky `availability` event with the snapshot:
  once after the plugin chain (with the capabilities freeze) and from
  `alloy_plugins/events.rs` on every `AlloyEvent::Availability`.
- `base_capabilities()` drops link, share and haptics;
  `register_capabilities` and `flux::capabilities()` drop dialog and
  clipboard-image. `navigator.share` is defined on every build (its call
  rejects with `NotSupportedError` where unavailable), so the surface no
  longer appears and disappears with a platform.
- `packages/flux-types`: `Flux.availability`, the `availability` event
  in `lattice/events.d.ts`, and the comments in `link`, `haptics`,
  `dialog`, `share` and `clipboard` that said "listed in
  Flux.capabilities only where".

lattice: the control API `/availability` (GET, per client) answering the
snapshot; the `info` message carries `availability` beside
`capabilities` (the client's own facts, from ore); `sol:test` gets
`setAvailability(name, state)` for a test's engine. The MCP `list_clients`
and `get_dialog` descriptions and `packages/cli/agents/debugging.md`
follow; the console's server panel shows the states beside the list.

core: `availability(name)` in `packages/core/src/environment.ts`
(a signal fed by the sticky event, `Flux.availability(name)` before the
first emit), exported from the index; `capabilities.haptics` reads it;
the `openLink` comment and the dialog types say "offer the feature unless
unavailable and handle NotSupportedError". The core AGENTS topic
(`agents/haptics.md` or a new `agents/availability.md`) states the three
questions and that rule.

Tests: ore unit tests in `ore/src/tests/availability.rs` (set, change
detection, the call-corrects rule through the dialog and link sessions);
flux module tests for `Flux.availability` and the override under the
test host; the `link`, `share`, `haptics`, `dialog` and clipboard tests
that asserted a capability name assert `"available"` instead; an app
test that `availability("dialog")` updates when the test sets it.

Closes [android-tv-dialog-stub](../backlog/android-tv-dialog-stub.md)
and [file-dialogs-no-picker-device](../backlog/file-dialogs-no-picker-device.md)
once verified on the TV and the tablet.

### Step 3: device availability for camera, microphone and audio output

- ore: Camera `Available` when `devices()` is non-empty once `start()`
  reports Ready, `Unavailable` when empty, re-read on
  `CameraDeviceChange`. Microphone and audio output from
  `SDL_GetAudioRecordingDevices` and `SDL_GetAudioPlaybackDevices` once
  the audio subsystem is up, re-read on SDL's audio device added and
  removed events (translated in `platform/translate.rs`, which does not
  pass them on today). `Unknown` before the subsystem is up.
- The touchscreen line of okf/tiny.md by the same rule: a touch sample
  marks touch present and re-emits `InputDevices`.
- Verification on a desktop without a webcam and with one plugged in,
  and on the tablet.

### Step 4: `navigator.permissions`

- ore: `Permission { Camera, Microphone }` (haptics and internet are
  declarations, never asked for; see section 4),
  `PermissionState { Granted, Denied, Prompt }`, `state(Permission)`,
  `request` answering with a state, a change queue through the pump like
  availability, and the state re-read on `DidForeground`. Android: the
  asked-before bit per permission in the activity's SharedPreferences
  through a JNI helper, cleared when granted. macOS:
  `AVCaptureDevice.authorizationStatus` through objc2. Linux outside a
  sandbox: Granted. Windows: Prompt until the open decides (to be
  checked). The mapping per platform is written in `ore::permission`'s
  module doc, one contract.
- flux `standards_plugins/permissions.rs`:
  `navigator.permissions.query({ name })` resolving to a `PermissionStatus`
  with `state`, `onchange` and `addEventListener("change")`; the names
  are the web's (`camera`, `microphone`); any other name, and a name
  whose build fact the project declared away, rejects with a `TypeError`.
- The camera and microphone sessions: a `Denied` open settles the
  permission to `denied`. `sol:test` gets `setPermission(name, state)`;
  the control API and the dev client's info carry the states.
- core: `permission(name)` reactive read beside `availability(name)`.

### Dependency decision for step 2b

The introspection needs a D-Bus client in ore. The candidates: `zbus`
(pure Rust, the dbus2 organization, used by GNOME tooling and ashpd;
pinned exact, no default features, `tokio` feature), or `dbus` (dbus-rs
over libdbus-1, battle-tested, adds libdbus-1 headers to every Linux
build and CI). Decided: `zbus`, pinned exact, for not adding a system
build dependency; it lands as step 2b, after 2a is reviewed, and until
then the portal-alone case reads `unknown` and the first call settles
it, which is already honest.

## Findings

- Step 1 (2026-11-15): the capability list is a per-engine registry in
  flux, frozen after the plugin chain. The native report
  (`flux::capabilities()`) is the build's list; the app's exclusions are
  not subtracted there, since a dev client runs many apps on one
  connection. The `uses` set reaches the runtime in the manifest from all
  three sources (go store, packed payload, `--assets` folder) through
  `Manifest::excluded_capabilities`; `sol test` stages a manifest, so a
  fixture project proves it end to end
  (packages/cli/tests/fixtures/uses-app).
- Step 2 (2026-11-15): a Linux session with the portal and no zenity (this
  desktop) reads dialog `unknown` and link `available` (xdg-open on the
  path). SDL's unix `SDL_OpenURL` has no distinct "no opener" error (it
  tries the portal, then spawns xdg-open), so the link Unavailable
  correction fires only through the test opener; the Ok-on-Unknown half
  fires for real. Android `showDialog` returning false stays
  `DialogError::Failed`: it can be one intent's answer (a folder picker
  missing), not the facility's.
- An excluded declaration (`uses.haptics: false`) reads `unavailable` from
  `Flux.availability` whatever the device has, so the show-unless-
  unavailable rule hides it; haptics is a declaration that is no longer a
  build-list name, and the exclusion registry carries it to the plugin.
- Solid 2.0: a signal written during a sticky event's synchronous replay
  reads its committed value until the next flush, so core's
  `availability()` answers from `Flux.availability` on that first read.
- The dev client's `info.availability` is the client's own facts at
  connect; the live map with test overrides is the `availability` query
  (`/__control__/availability`).
- Step 4 (2026-11-15): on Android a request's refused answer records the
  same mapping the foreground re-read uses (granted, rationale, asked
  bit), so a first denial reads `prompt` (a request still shows the
  dialog) and `denied` only after a final one, whichever of the answer and
  the resume lands last. The request's callback still carries the
  request's own outcome, so a refused microphone open fails while the
  standing state reads `prompt`. The test engine pins both permissions to
  `granted`, so app tests read the same on every machine. The Windows
  switches are read through `windows-registry`, the macOS status through
  `objc2-av-foundation` 0.3.2; neither was compiled here (no target std),
  nor was the Android Java. zbus is pinned at 5.19.0: the 6.x line is a
  month old and rests on a new runtime crate. The portal introspection
  took 22 ms cold and 1 ms warm on this desktop.
- The ore unit tests go through the main-thread bridge, and SDL counts as
  the main thread whichever test thread first initialized it, so a bridged
  call from another test would queue forever; in ore's own unit tests the
  bridge runs the call at once (`main_thread::run`, `cfg(test)`).
- Still to verify on devices: the TV's dialog reads `unavailable` (the
  stub check in `SolidRTActivity.dialogsAvailable`, unbuilt here) and the
  tablet's `available`.
- Step 3 (2026-11-15): the camera and the audio subsystems both start
  lazily (the camera at the first list or open, the audio at the first
  sound, output-rate read, or microphone list or open), so camera,
  microphone and audio read `unknown` until an app uses them, the "only a
  call can tell" case; then the device lists settle them. On this desktop
  (no webcam, two capture inputs, three sinks) a probe that lists both and
  reads the output rate reads camera `unavailable`, microphone and audio
  `available`; hello-world alone reads all three `unknown`. On Android,
  SDL's AAudio backend lists one default recording device whatever the
  hardware and the RECORD_AUDIO permission, so the microphone reads
  `available` there once the subsystem is up, also on a device with no
  microphone.
