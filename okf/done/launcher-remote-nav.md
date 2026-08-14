---
title: Launcher remote navigation
description: "Pointer-free control of the launcher for TV: a launcher-local spatial focus registry driven by arrow/Select key events and gamepad dpad edges, plus a separate fix for mouse clicks dying on Android TV."
created: 2026-07-28
completed: 2026-07-28
---

# Launcher remote navigation

Make the launcher drivable without a pointer, so a TV remote (dpad),
a hardware keyboard, or a gamepad can operate it. Prompted by the go
client on an Android TV: the remote did nothing, and an attached mouse
moved (OS-drawn pointer + SDL hover motion) but its clicks never
arrived (separate bug, stage 2).

## Background

The input plumbing already existed end to end and had zero consumers:

- SDL gamepad+joystick subsystems are initialized in alloy
  (`alloy/src/gamepad.rs`), snapshots coalesce to one sticky
  `gamepads` event per main-loop iteration, and core exposes the
  reactive `gamepads()` accessor (`packages/core/src/gamepad.ts`).
- Key events reach JS (`keydown`/`keyup` on the `srt:events` bus) but
  core dispatches them only to the single focused node
  (`packages/core/src/window.ts`); there is no focus traversal, and
  `Pressable`/`Button` cannot be activated by key.
- A TV remote arrives as key events (dpad = arrows; center =
  `code: "Select"`, whose `key` is "Unidentified" - match by code), NOT
  as a gamepad. A game controller arrives via `gamepads()`. Both must
  be handled.

## Stage 1 - launcher-local nav layer (implemented 2026-07-28)

`lattice/launcher/parts/nav.tsx`: a module-scope registry (the
dev-connection singleton pattern) of the mounted screens' pressable
targets.

- `navTarget(action, {modal?, disabled?})` registers in a component
  body, unregisters via `onSettled` cleanup; returns `ref` (bounds)
  and `focused()`.
- Movement is spatial, not registration-order: at press time each
  target's `getBoundingBoxViewport` center is taken, the nearest
  target with progress along the pressed direction wins (cross-axis
  distance weighs double). Entry focus = topmost-leftmost. So mount
  order never matters.
- Modal targets (the exit and remove confirm dialogs) trap
  navigation while mounted.
- Inputs: `on("keydown")` (arrows move, Enter or `code === "Select"`
  activates; ignored while `getFocusedNodeId() != null` so TextInput
  typing is untouched) and `on("gamepads")` (edge-detected dpad +
  south across the union of pads).
- Nothing is focused until the first navigation press; pointer input
  is unchanged. First activate lands focus instead of acting.
- `navRing(focused, radius?)` = 2px text-colored border (text, not
  primary, so it shows on primary-filled buttons), spread into the
  target's existing style. `NavButton` is the Button drop-in
  (components' `Button` gained a `ref` prop for it).

Wired sites: app cards, header gear, DevCard buttons, detail
Launch/Remove/Clear cache, both confirm dialogs (modal), BackButton,
settings SegmentedControl (one target; activate cycles the mode),
connect screen buttons + recents, scan close.

Deliberate stage-1 gaps:

- No scroll-into-view: a focused off-screen app row stays off-screen
  (ScrollView has no external scroll API today).
- No held-dpad auto-repeat on gamepads (keyboard repeat works).
- TextInputs are not targets (manual address entry needs a pointer or
  hardware keyboard).
- The app rows' nested play button is skipped: activate opens the
  detail, Launch lives there.
- Gamepad east is not mapped to back (remotes have a real Back key,
  which already works via the back event).

Verified: `srt check` clean, bundle + Linux client rebuilt. NOT yet
verified on a device; desktop keyboard (arrows/Enter) exercises the
same paths.

## Back triggers (implemented 2026-07-28)

With only a gamepad there was no way to leave a running app, and the
launcher's back stack was unreachable. Wired natively in alloy, so it
works everywhere (launcher and running apps) with the existing back
contract (onBack can preventDefault; unhandled back exits to
launcher):

- `is_back_trigger`: AC_BACK is now a back trigger on ALL platforms,
  not just Android - the desktop keyboard's BrowserBack media key
  arrives as the same scancode. This reverses the desktop carve-out
  documented in `okf/plans/exit-to-launcher.md` (deliberate, user
  request).
- Gamepad "back" (select) button: `Gamepads::take_back_edge` in
  `alloy/src/gamepad.rs` level-reads the mapped pads' Back button once
  per main-loop iteration and emits `AlloyEvent::Back` on the press
  edge (after the snapshot send, `app.rs`). The snapshot stays
  FAITHFUL - "back" is reported like any button (an initial
  withholding was reverted per the no-core-magic principle); the back
  event carries the intent and apps that bind "back" preventDefault
  it. Raw HID pads trigger nothing - "back" there is a positional
  guess, too uncertain to hang an exit intent on. Chosen over "start"
  (pause menu in most games) and "guide" (Android intercepts it);
  Escape deliberately NOT mapped (app-owned meaning).

## TV remote back - diagnosed and fixed (2026-07-28, device-unverified)

Logcat verdict: `key code=4 action=0 dev=4 src=0x301 joy=true` - the
remote's Back is KEYCODE_BACK from a joystick-classified device that
IS one of the opened mapped pads. SDL forwards it as raw joystick
button 4 (Android sends gamepad-button enum values as joystick
indices, and nbuttons = highest mask bit + 1, so dpad bits keep index
4 in range), but the pad's auto-generated mapping lacks the back
entry: the mask is built from a `hasKeys(KEYCODE_BACK)` probe that TV
remotes routinely fail, so `pad.button(Button::Back)` never goes true.

Fix (alloy/src/gamepad.rs): a mapped pad now also keeps a raw
joystick handle to the same device (SDL refcounts the open), and on
Android `take_back_edge` additionally reads raw button 4. Elsewhere
raw indices are device-arbitrary, so only the mapping is trusted.
Bonus: any joystick-classified device sending KEYCODE_BACK (e.g. a
keyboard/mouse dongle that got opened as a pad) lands on the same
back intent.

## Superseded analysis (kept for the record)

First TV round: remote dpad/center navigate (they arrive as GAMEPAD
input: SDL's Java layer classifies any SOURCE_DPAD device - every TV
remote - as a joystick, `SDLControllerManager.isDeviceSDLJoystick`),
but remote Back did nothing. Findings from the SDL 3.4.10 source:

- `keycode_to_SDL(AKEYCODE_BACK)` = SDL_GAMEPAD_BUTTON_BACK, so the
  remote's Back SHOULD surface as the pad "back" button and hit
  `take_back_edge` already. That it does not points at the pad being
  Raw on our side (take_back_edge is mapped-only, and a raw remote
  would also misname dpad, which nav's working dpad contradicts), or
  the SDL auto-mapping omitting the back entry (Java's button_mask
  may not include KEYCODE_BACK). The `key code=4 ... joy=` diagnostic
  line plus alloy's "pad connected (mapped|raw)" line will decide.
- `SDL_TV_REMOTE_AS_JOYSTICK=0` is NOT a fix: with the hint off,
  SDL's `button_to_scancode` turns the remote's Back into ESCAPE (not
  AC_BACK), and Escape is deliberately app-owned.
- A Java-side KEYCODE_BACK reroute onto the keyboard path was tried
  and REVERTED: vendored SDL Java stays vanilla (user policy); only
  temporary marked diagnostics may live there, and the fix belongs in
  Rust once the diagnosis lands.

Also: log chatter on the TV drowned diagnosis. Two causes: (1) the
steady-state pacing warns (slow frame, present fence timeout, vsync
signal missed) were warn-level - demoted to debug (counters/stats
remain); (2) the go MainActivity carried a TEMPORARY block from the
swap-latency diagnosis forcing SRT_LOG=debug on every Android launch,
which surfaced every debug trace (and masked the demotions) - now
converted to an opt-in `srt_log` intent extra (`adb shell am start ...
-e srt_log debug`). alloy additionally logs one line per pad connect
with mapped/raw classification (permanent).

## Stage 2 - TV mouse clicks (open)

Pointer moves but clicks do nothing on the TV. Motion reaches SDL via
hover generic-motion events; a click goes through
`SDLSurface.onTouch` -> `onNativeMouse(event.getButtonState(), ...)`
and SDL's native side diffs the button mask - many Android TV boxes
report `getButtonState() == 0` on click, so no button event is ever
posted. The event dies in the vendored SDL Java layer, before Rust.

Round-1 "no output for clicks" was a stale APK. With the real
diagnostics in place, logcat shows clicks flowing CORRECTLY through
Java: `onTouch src=0x2002 tool=3 action=0 buttons=1` (ACTION_DOWN,
BUTTON_PRIMARY) and the matching ACTION_UP, plus the
BUTTON_PRESS/RELEASE (11/12) generic-motion echoes SDL ignores by
design. So the earlier buttonState-0 theory is DEAD: the click
reaches native SDL with the primary button set, and the bug is
further down the pipeline.

Corner-click probe: coords arrive true in PHYSICAL pixels ((0,0) and
(1908,1062) against window (1920,1080)), and the user then observed
hover highlighting elements BELOW the pointer. Diagnosis complete:
alloy's resize event defines JS logical space as pixels /
display_scale (1280x720 on this density-1.5 TV) and the renderer
paints that tree x1.5, but mouse events passed through UNSCALED on
the assumption that SDL window units are logical - true on macOS/
Wayland, FALSE on Android/X11 where they are physical pixels. So the
0..1080 mouse landed in a 0..720 tree: hover hit ~1.5x lower, clicks
in the lower/right third hit nothing. Touch is normalized, so phones
never showed it.

Fix (2026-07-28, device-unverified): `mouse_scale(window)` in
alloy/src/event.rs - mouse coords multiply by
(size_in_pixels/display_scale) / window.size(), the ratio between the
JS logical space and SDL's window units. 1.0 where SDL is already
logical, 1/display_scale where physical; derived from the same sizes
the resize event uses, so mouse and layout agree by construction on
every backend, no target cases. Applied to motion, down, up, and
wheel position (deltas untouched).

TV-verified 2026-07-28: mouse hover and click, remote dpad/center
navigation, remote Back, and the desktop Ctrl+Shift+Backspace chord
all work. All temporary diagnostics removed (3 SOLIDRT-INPUT Java
sites + the alloy mouse-down probe); the vendored SDL Java is
byte-identical to vanilla again.

Still open from stage 1 (deliberate gaps, unchanged): scroll-into-view
for off-screen nav targets, gamepad dpad auto-repeat, TextInput nav
targets, components-level promotion of the focus layer.

## Later

- Promote focus traversal into `packages/components` as a reusable
  layer (known gap, `okf/notes/core-package-review.md`); the
  launcher layer is the prototype.
- Scroll-into-view once ScrollView grows an external scroll API.
