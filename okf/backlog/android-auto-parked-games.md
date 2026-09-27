---
title: Android Auto parked games
description: Since Android 15, Android Auto runs a parked game as an ordinary activity on the car display, drawing its own UI, so a solidrt game can reach car screens on the renderer it already has; what is missing is a per-app manifest opt-in through srt pack, a Play upload path, and a verification pass on the Desktop Head Unit for the second display, the density change and the driving block.
created: 2026-09-27
---

# Android Auto parked games

A solidrt app cannot appear on a car screen today. Android Auto (the phone
projecting to the car's head unit) mostly does not let an app draw: media,
messaging, navigation, points of interest, weather and IoT apps hand data to
host-rendered templates (the Car App Library, a media session,
notifications). The exception, on phones running Android 15 (API 35) or
later, is the parked-app category: a game runs as an ordinary activity
launched on the car's display, draws whatever it likes, and the system blocks
it once the car moves. The category left beta in September 2026. Games are the
only parked category Android Auto opens to third-party apps; video and
browsers are listed for Android Automotive OS only.

That fits solidrt as it is: the activity is our SDLActivity and the renderer
is the one we ship. The work is packaging and a verification pass, not a new
rendering target.

Done looks like: a packed game that opts in through package.json shows on the
Android Auto launcher, fills the car display at its sizes without
pillarboxing, keeps its state when it moves between the phone and the car
display, goes hidden and silent when driving starts, and its bundle passes
Play's car app quality review for the Android Auto form factor. The go client
shows on the Desktop Head Unit (DHU) so apps can be tried there during
development.

## What the platform asks

- Manifest: `android:appCategory="game"` on `<application>`, and
  `android.intent.category.CAR_LAUNCHER` in the launcher activity's intent
  filter (the `LAUNCHER` filter or a separate one). No `distractionOptimized`
  metadata on any activity: its absence is what lets the system block the
  app while driving. `android.hardware.gamepad` with `required="false"` is
  recommended for a game that takes a controller.
- Play reviews it: a build declaring `CAR_LAUNCHER` submitted outside a
  permitted category is rejected. The declaration is per app, never in the
  shared runner.
- The car app quality guidelines for the game category, the ones that land
  on the runtime:
  - DD-2 / DD-3: not launchable or visible while driving; audio stops when
    driving starts and cannot be unpaused while driving.
  - EP-2: relaunched from the home screen, the app restores its previous
    state as closely as possible.
  - DO-2: not significantly pillarboxed on landscape displays. Android
    Auto's anchor ratios are 16:9 landscape and 9:16 portrait; the canonical
    test displays are 800x480 at 160 dpi, and 1920x1080 at 160 dpi in wide
    landscape and in portrait with cropped margins.
  - AR-1: interactive UI not under system bars or display cutouts.
  - EP-3 (no freezes or stutter), DR-2 (launch within 10 s), IN-2 (no
    heads-up notifications), AN-1 (no dead ends).
  - Tier 2 (car optimized) adds 64dp touch targets 24dp apart and 24sp
    text. Those are the app's concern.
- Detecting the car, only if an app wants to adapt: `CarConnection` from
  `androidx.car.app` reports a projection connection, and the activity's
  display is not `DEFAULT_DISPLAY`. Google documents the combination as not
  always accurate.

## What exists already

- The lifecycle contract EP-2 needs: `env.visibility`, `onSuspend` and
  `env.launch` (`okf/done/app-lifecycle-events.md`,
  `okf/done/app-suspend-quit-hooks.md`). Restoring is the app's part.
- Audio: every sound goes out through SDL's device (flux:audio through
  SDL_mixer, video audio through its sink in `forge/src/video/audio.rs`), and
  SDL pauses its Android devices when the activity pauses with block-on-pause
  on (`Android_OnPause` in SDL's `SDL_androidevents.c`). If the driving block
  pauses the activity, the audio half of DD-2 already holds.
- The safe area (`packages/core/src/environment.ts`) for AR-1, and a display
  scale change flows through `AlloyEvent::Resize { display_scale }`.
- SDL's display lookups go through the activity's WindowManager, whose
  `getDefaultDisplay()` returns the display the activity is on, not the
  primary one, so metrics and DPI follow the activity onto the car display.

## Work

### 1. Go client on the DHU

Declare `appCategory="game"` and `CAR_LAUNCHER` in the go overlay
(`lattice/android/app/src/go/AndroidManifest.xml`). The per-app rule binds
Play submissions under the Android Auto form factor, which the go client is
not. Android Auto lists a sideloaded app only with "Unknown sources" on in
its developer settings. Every later step is verified on this.

### 2. Verify on the DHU

Each of these is a question for the running client, not a design:

- **Second display.** The display listener in `SolidRTActivity.java`
  (`onDisplayChanged`) returns early for every display but
  `DEFAULT_DISPLAY`, so on the car display a refresh-rate change never
  re-pushes. It should compare against the activity's own display, re-read
  on each call since the activity can move.
- **Moving between displays.** With `launchMode="singleInstance"`, launching
  from the car launcher while the app runs on the phone probably moves the
  task to the car display. Density changes with it (a phone near 420 dpi,
  the car at 160), and the manifest's `configChanges` does not list
  `density`, so Android would recreate the activity, which for SDL is a
  native restart that keeps only what `onSuspend` persisted. Find out what
  happens. The likely fix is adding `density` and checking that the display
  scale Resize path handles a live change on Android, so the move is a
  resize, not a restart.
- **Driving.** Confirm the block reaches us as an activity pause (SDL pauses
  audio, `env.visibility` goes hidden) and that nothing resumes until
  parked.
- **Input.** Touch arrives as touch. How a rotary controller reaches a
  parked activity on Android Auto is to find out (the DHU emulates one); if
  it arrives as key events it joins the path of
  `okf/backlog/remote-control-input-device.md`.
- **Insets.** Whether the car display hands the window system bar or cutout
  insets, and whether the safe area reports them.
- **Game Mode.** `appCategory="game"` also makes Android treat the app as a
  game on the phone: Game Mode and the manufacturer interventions it allows
  (downscaling, frame rate override) apply unless an
  `android.game_mode_config` resource opts out. Check whether either
  intervention touches our frame pacing, and if so ship a config with both
  off for packed games.

### 3. Opt-in through package.json and srt pack

Per the packaging rule (what the app is goes top level and platform-neutral;
the `android` group holds only what Android alone needs), both facts are
intentions: the app is a game, and it wants to be on car screens. A shape to
decide:

```json
"solidrt": {
  "category": "game",
  "car": true
}
```

`category` maps to `android:appCategory` here and to the equivalent on
other packers later (macOS `LSApplicationCategoryType`). The same attribute
puts a game in the Android TV launcher's Games row
(`okf/backlog/android-tv-play-distribution.md`). `car` maps to
`CAR_LAUNCHER` for Android Auto, and to the automotive `uses-feature` once
Automotive OS is in scope. While games are the only category Android Auto
opens, `srt pack` refuses `car` without `category: "game"`. As with `tv`
(`okf/backlog/android-tv-play-distribution.md`), knowing the intent also
lets pack print the Play Console steps after a `car: true` AAB: the
Android Auto form-factor opt-in, an open testing track, the quality review.

The patcher (`packages/cli/src/pack/android/strings.ts`) today inserts
`<uses-permission>` elements and flips values in place (backup). This needs
two edits it cannot make yet:

- An attribute inserted on `<application>` for `appCategory`. An `android:`
  attribute needs its resource id in the manifest's resource map, and
  attributes are sorted by that id. There is no neutral enum value the
  runner could declare for an in-place flip.
- A `<category>` element inserted into the launcher intent filter: element
  insertion like `addUsesPermissions`, but nested.

Declaring the filter in the runner and disabling it (the backup trick) is
not a way out: Play reads the declared manifest, so every packed app would
likely count as declaring `CAR_LAUNCHER`.

### 4. Play

Depends on `okf/backlog/play-store-aab.md`: outside developer mode Android
Auto lists only Play-installed apps, and Play takes bundles. The AAB patch
then makes the same two edits in the proto manifest. After that: opt in to
the Android Auto form factor in Play Console, submit to an open testing
track, pass the quality review.

## Android Automotive OS

Cars with Android built in run the app natively, and the same packaging
work reaches them: `<uses-feature android:name="android.hardware.type.automotive" android:required="false"/>`
on the phone track (`required="true"` only on a dedicated Automotive OS
track), the same `appCategory`, the same driving rules. The differences:
video and browser apps are allowed there as well, a wider fit for solidrt
than Android Auto; the anchor ratios are 4:3 landscape and 10:16 portrait;
screens may be fixed-orientation (DO-1); and deep links count toward the top
tier (DL-1; the prod runner already answers the app's scheme). Testing is on
the Automotive OS emulator. Not part of done here; its own item once the
Android Auto path works.

## Not in scope

- The templated categories (media, messaging, navigation, points of
  interest, weather, IoT). The host draws them from data; supporting them
  means a Solid renderer that emits Car App Library templates, plus a service
  that runs without SDL's activity. It uses none of the renderer, so it waits
  for an app that needs it.
- An `env` fact for "in a car". Review does not need it, and `CarConnection`
  would add the `androidx.car.app` dependency to the runner. Revisit when an
  app wants car-specific UI.

## Open questions

- The config names (`category`, `car`).
- Whether launching from the car launcher moves the running task or starts a
  fresh one, which decides how much the density work matters.
- How to put the DHU into a driving state.

## References

- [Add support for Android Auto to your parked app](https://developer.android.com/training/cars/parked/auto)
- [Build games for cars](https://developer.android.com/training/cars/parked/games)
- [Build parked apps for cars](https://developer.android.com/training/cars/parked)
- [Car app quality guidelines](https://developer.android.com/docs/quality-guidelines/car-app-quality)
- [Bring your Android game to the car screen today](https://android-developers.googleblog.com/2026/09/bring-android-game-to-car-screen.html)
