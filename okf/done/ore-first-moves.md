---
title: User folders, thread placement, the Android glue, power and the theme moved into ore
description: Landed 2026-11-12 - the pref path and home dir (`ore::folders`), power (`ore::power`), the system theme (`ore::theme`, the first resident of ore's new `window` feature), thread placement (`ore::threads`, named by what the OS gives - `Tier::Deadline`, `Tier::Interactive`, a burst worker), the Android JNI glue (`ore::android`, with a publish of ndk-context that runs once and that every activity call makes itself) and the permission request, one call on every platform (`ore::permission`) left alloy, lattice and forge; alloy and forge gained their first edge to ore, alloy lost windows-sys and ndk-context, forge lost ndk-context, and lattice no longer calls SDL for its storage roots.
created: 2026-11-12
completed: 2026-11-12
---

# User folders, thread placement, the Android glue, power and the theme moved into ore

## Outcome

What landed, where it differs from the item as shaped below:

- **New edges.** Neither alloy nor forge depended on ore before. alloy now
  does, with the `window` feature; forge does on every platform, because
  `forge::process::home_dir` stays as a one-line delegate to
  `ore::folders::home_dir`. `flux:process` is a forge plugin and a forge
  plugin marshals forge only, so the plugin did not change.
- **The `window` feature exists**, with the theme alone in it. Headless
  `flux` links ore without it.
- **Events name alloy.** `PowerInfo`, `PowerState` and `SystemTheme` are
  ore's types, re-exported at alloy's root beside `AlloyEvent`, so flux's
  alloy layer does not name ore.
- **ore names what the OS gives.** `ore::threads::raise(Tier::Deadline |
  Tier::Interactive)` and `place_burst_worker() -> Placement`; alloy maps
  its raster, UI and sort threads onto them.
- **The Android context publishes itself.** The packed entry looks up its
  payload before alloy starts, and ndk-context asserts on a second
  publish, so `ore::android::init_context` runs once behind a `OnceLock`
  and `with_activity` makes the call itself. alloy still calls it at
  startup, for the dependencies that read ndk-context without going
  through ore.
- **One attach path.** `has_touchscreen_feature` (not in the item) and
  `packed_asset_location` moved onto `with_activity`; forge's content
  providers use ore's `with_activity` and `java_strings`. Both are public
  and hand out `jni` types, which no ore contract does: they go back to
  crate-private when the dialogs, the video plane, the redraw answer and
  the content providers move.
- **`ore::permission::request(Permission::Microphone, closure)`** is the
  same call on every platform: Android prompts, the others answer yes at
  once. No SDL type and no Android name crosses, and alloy's microphone
  has one path with no `cfg`: ask, open when granted, at the open itself
  when the answer is already in. One variant today; the model for the
  camera, location and the rest is still to be designed.
- `home_dir` still reads the environment variable; the
  `SDL_GetUserFolder` probe on Android is open.

Verified on Linux: `cargo test -p ore` (with and without `window`), the
alloy, flux and lattice tests and `sol test`. Verified on an Android tablet
(arm64) with the go client and a packed APK: the raster thread runs at
nice -8 and the UI thread at -4; the microphone permission prompt shows,
its answer reaches the closure and the device opens, and a second open
with the permission held opens without a prompt; a file dialog opens
the system picker and its cancel answers the app; the packed APK finds its
payload before alloy starts and draws. Not exercised: a picked document
(`java_strings` and the content providers through ore), and the macOS and
Windows thread paths, which were not built.

## The item as shaped

okf/design/platform-layering.md says existing OS code moves to `ore` "as
features touch it, not before". These three pieces are the cheapest of what
is left, none needs a design decision, and each has a reason to move now
beyond tidiness. They are one item because they share the same moves (a new
ore module, callers switched, a dependency leaving a crate) and can land as
three commits in any order.

Done looks like: no `get_pref_path` call outside ore; `sdl_utils.rs` holds
none of the thread placement, Android or power and theme code; no crate
outside ore and alloy names `alloy::sdl_utils` for an OS fact; the Linux
tests, `make client`, and on the Android tablet a microphone permission
request, a dialog and a packed-APK start all behave as before.

## 1. User folders

Where it is today:

- `lattice/src/storage.rs` (`pref_dir`) and `lattice/src/gl_libs.rs` both call
  `alloy::sdl3::filesystem::get_pref_path("SolidRT", app)`. They are the only
  `get_pref_path` callers.
- `forge/src/process.rs` `home_dir()` rebuilds a slice of `SDL_GetUserFolder`
  from `HOME` / `USERPROFILE`; `flux/src/forge_plugins/process.rs` marshals it
  as `homedir()` in `flux:process`.

The move: an `ore` module for the per-user locations, with `pref_dir(org,
app)` and `home_dir()`. SDL's filesystem functions are any-thread and need no
`SDL_Init`, so headless `flux` can call them (okf/design/platform-layering.md,
"SDL thread rules").

- The `SolidRT/<app>` layout, `safe_component` and the `client`, `data`,
  `identity` and `logs` subdirectories stay in lattice: they are the storage
  policy. Only the SDL call moves, and ore takes the org and app names as
  plain strings, as SDL does.
- `home_dir` moves with its current behavior (the environment variable). Whether
  `SDL_GetUserFolder(SDL_FOLDER_HOME)` can replace it has to be probed on
  Android first, where HOME is not guaranteed; do not swap it blind.
- forge's `home_dir` goes away and `flux:process` marshals ore's. forge
  already may depend on ore (without `window`), so this adds no edge.
- `lattice/src/tests/storage.rs` talks about the real pref path (its own
  comments at lines 46 and 128); check it still points at the right thing.

What stays: the macOS-only `alloy::sdl3::hint::set` in `gl_libs.rs` is an SDL
hint for the GL library load, which belongs to the window part of ore. It
moves with the pump, not here, so lattice still names `alloy::sdl3` once
afterwards, on macOS.

Payoff: lattice's storage roots stop reaching through alloy for SDL, and the
"headless `flux` has no per-app data directory" gap in the layering note has
a facility to close it with. A JS surface for it is a separate feature (it
needs the app identity decision) and is not in this item.

## 2. Thread placement and the Android glue

Both live in `alloy/src/sdl_utils.rs`, and neither is SDL.

### Thread placement

`frame_thread_priority`, `latency_worker_thread_priority`, `LatencyPlacement`,
`thread_nice`, `thread_util_min`, `performance_cores` and `pin_thread_to` are
libc on Linux and Android, `pthread_set_qos_class_self_np` on macOS and
`SetThreadPriority` on Windows. The only callers outside `sdl_utils.rs` are
`alloy/src/threads.rs` (`frame_thread_priority`, the raster and UI threads)
and `alloy/src/gpu/order_worker.rs` (`latency_worker_thread_priority`, the
sort worker).

The move: an `ore` thread module exposing two things, the frame priority
(critical or frame-building tier) and the latency worker placement with its
`reassert`. `thread_nice`, `thread_util_min`, `performance_cores` and
`pin_thread_to` become private to ore, as nothing else calls them. This keeps
the split the layering note wants: alloy decides which thread is which tier,
ore carries it out.

- `frame_thread_priority` has the nice values `-8` and `-4` inline. They are
  tuning literals; when the function moves they become named constants with a
  comment, as the root CLAUDE.md requires (the latency worker's two are
  already named).
- The long doc comments carry the measurements behind the numbers; they move
  with the code. Log lines change from `[alloy]` to `[ore]`.
- ore gains `libc` (unix) and `windows-sys` (`Win32_System_Threading`).
  `alloy/Cargo.toml` says its `windows-sys` entry exists only for
  `frame_thread_priority`, so that dependency leaves alloy; `libc` stays there
  (`touch.rs`, `vsync.rs`, `frame_timestamps.rs` use it).

### Android glue

- `with_android_activity`, `java_strings`, `init_android_context`,
  `packed_asset_location`, `request_android_permission`.
- Callers: `video_plane.rs`, `dialog.rs` (twice, plus `java_strings`) and
  `redraw.rs` call `with_android_activity`; `microphone/mod.rs` calls
  `request_android_permission`; `app.rs` calls `init_android_context` at
  startup; `lattice/src/entry.rs` calls `packed_asset_location` through
  `alloy::sdl_utils`, an OS fact reached through the window crate.

The move: all five go into `ore/src/android.rs`.

- `ore::android::with_activity` already exists with the same shape as alloy's
  `with_android_activity` (it reads the JavaVM and activity from ndk-context).
  Two copies of the attach-and-wrap pattern is the accident; keep ore's, switch
  the alloy callers to it. forge's `fs/content.rs` has a third (including its
  own `java_strings`); switching it to ore's is the same change, and
  recommended in this item rather than left, since forge may already call ore.
- `init_android_context` is the call that publishes the JavaVM and activity in
  ndk-context. The layering note requires the windowless entry to do exactly
  that, with no alloy running. It must be an ore function before the windowless
  host can be built; alloy keeps calling it at startup meanwhile.
- `request_android_permission` takes the raw
  `SDL_RequestAndroidPermissionCallback` and a `userdata` pointer in its public
  signature. In ore that becomes a Rust closure (`FnOnce(bool) + Send`) so no
  SDL type crosses the boundary; `microphone/mod.rs` adapts. This item moves
  the wrapper only. The one permission model across camera, microphone,
  location, notifications and exact alarms (layering note, "Permissions") is
  its own design and stays out.

## 3. Power info and the system theme

`get_power_info`, `PowerInfo`, `PowerState`, `system_theme` and `SystemTheme`
in `sdl_utils.rs`. The two behave differently, which decides what moves and
how:

- Power (`SDL_GetPowerInfo`) is window-free and not thread safe: one caller
  at a time. It becomes an ore module behind a lock, and headless `flux` can
  call it.
- The theme (`SDL_GetSystemTheme`) returns the video device's cached value
  and `UNKNOWN` when no video subsystem exists (`SDL_video.c`, SDL 3.4.18). So
  the theme belongs to the `window` part of ore and reads `Unknown` headless.
  Say so in its doc comment; do not present it as a window-free fact.

Consumers: `alloy/src/event.rs` carries `PowerInfo` and `SystemTheme` in
`AlloyEvent`; `alloy/src/app.rs` polls power every `POWER_CHECK_PERIOD` and
sends the theme event from the SDL event; `flux/src/alloy_plugins/events.rs`
imports `alloy::sdl_utils::{PowerState, SystemTheme}` to map them to the
`"onBattery"` and `"dark"` strings core reads. The types move to ore, alloy's
events carry ore's types, and flux imports them from ore. The string mapping
stays in flux (marshalling). The polling and the event translation stay in
alloy's loop until the pump moves; only the query and the types leave.

Not decided here: a pull query from JS (the layering note lists Battery
Status for power; today both are events only, `packages/core/src/environment.ts`
reads `systemTheme` from the event). After the move that is a plugin-only
change, and its web shape and simplified contract are the next item's.

## Order and checks

Any order works. Suggested: 1, then 3 (power only, the theme with the window
part), then 2, because 2 is the one that needs the Android device.

- Per piece: `cargo test -p ore`, `cargo test -p alloy`,
  `cargo test -p flux --lib --features gui`, `bun run sol test`.
- Piece 2: `make android-client` and on the tablet, a microphone permission
  prompt, a file dialog and a packed APK start (the three callers of the
  glue). The thread placement has no test; check that the raster and UI
  threads still log their priority and that desktop Linux still logs the
  denied nice at debug, unchanged.
- After each piece: `grep -rn "sdl_utils::<moved name>"` finds nothing, and
  the unused dependency is removed from the manifest it left.

## Not in this item

- The pump, window, cursors, keyboard presence, `has_keyboard` /
  `has_mouse`, gamepads, dialogs (the SDL-main-thread half): these are the
  `window` feature and the biggest single move.
- The audio and camera device wrappers in `sdl_utils.rs` (they wait on the
  mixer split).
- forge's content providers beyond the `java_strings` / activity-attach
  duplicate above, and its MediaCodec JNI.
- Any JS surface for user folders, power or the theme.
