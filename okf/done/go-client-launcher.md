---
title: Go-client launcher
description: "The default app becomes the client's compiled-in launcher: version-store apps with tap-to-launch and delete, manual address entry, and a boot rule that drops the auto-boot."
created: 2026-07-21
completed: 2026-07-21
---

# Go-client launcher

Turns the go client's "default app" (today: the dev-server connect
screen) into the client's launcher: the home screen that lists installed
apps, launches and deletes them, and still connects to (new) dev
servers. Builds directly on the version store and boot machinery from
`okf/plans/client-storage-updates.md` (stages 1-3); this plan gives the
store the UI it never had.

## Status quo (2026-07-21)

- `lattice/default-app/` holds `app.tsx` (connect screen: discover, QR
  pairing, recents, `launchAddress` auto-connect) + `logo.tsx`, and
  `bsod.tsx` (crash screen). Bundled by the lattice Makefile
  (`default-app-bundle` target, `bunx srt bundle`) into checked-in
  `.srt.js` files, embedded via `include_str!` in `lattice/src/lib.rs`.
  The connect screen and `EngineCmd::Stop` are `go`-gated (2026-07-21);
  the BSOD is embedded in every build.
- The connect screen's only privileged surface is `srt:dev`
  (`lattice/src/go/control.rs`): `connect`, `discover`, `stop`,
  `recents`, `launchAddress`, `available`, `canDiscover`.
- The version store (`lattice/src/go/store.rs`) has
  `apps/<id>/versions/<manifest-hash>/` + `state.json`, filled by
  dev-push-as-install. Boot from the store happens in exactly one
  place: launched with `--dev-server`, `load_last()` boots the
  last-installed app immediately (offline relaunch,
  `lattice/src/lib.rs` ui_thread) and the server's latched reload
  replaces it on connect.
- There is no UI over the store: no listing, no launch-by-choice, no
  delete. `EngineCmd::Stop` (server-sent) returns to the connect screen.

## Decisions

- The launcher stays compiled into the client binary and updates only
  with it. Rejected alternative: shipping it as a factory-seeded store
  install. Rationale: `srt:dev`/`srt:apps` are private, unversioned
  APIs between the bundle and the binary that ships it; independent
  launcher updates would turn them into versioned contracts. The
  launcher is also the fallback when store state is broken, so it must
  not depend on the store. Factory seeding (storage stage 4), if it
  happens, applies to apps only; the launcher is the UI that lists them.
- Who provides the app decides what boots. Started without an address
  (user opens the installed client manually, Android or desktop): the
  launcher, always, online or offline. Started with a dev-server
  address: the server provides the app via the latched push; the
  launcher (showing connect status) is what's on screen until the push
  lands, and the fallback if the server is unreachable. The `load_last`
  startup auto-boot is deleted in both cases; warm-start-into-last-app
  is gone as a feature, deliberately. Dev-push-as-install stays: it is
  what fills the launcher's list.
- Delete means full uninstall: the app's entire folder under the go
  apps root (versions + state + data sandbox), behind a confirmation in
  the launcher UI.
- Returning to the launcher from a running app without restarting the
  client is out of scope (needs a client-owned gesture/key; the escape
  hatch design is its own discussion). For now: close and restart.
- The BSOD stays a separate compiled-in screen in all builds (it is the
  screen for "everything else is broken, possibly including the store").

## Stages

### Stage 1: rename, no behavior change (done 2026-07-22)

- `lattice/default-app/` -> `lattice/launcher/`; `app.tsx` ->
  `launcher.tsx` (bundle `launcher.srt.js`); `bsod.tsx` stays alongside
  as the other compiled-in screen.
- `DEFAULT_SOURCE` -> `LAUNCHER_SOURCE`; `DEFAULT_APP_*` Makefile vars
  -> `LAUNCHER_*` and the `default-app-bundle` target ->
  `launcher-bundle` (root Makefile forwards the target name); all
  "default app" / "connect screen" comments in lib.rs, main.rs,
  go/session.rs, go/control.rs now say launcher.

### Stage 2: srt:apps + launcher UI (done 2026-07-22)

- New focused module `srt:apps` next to `srt:dev` (dev-connection
  concerns stay in `srt:dev`):
  - `list()`: installed apps from the store; id, display name (installed
    manifest's `displayName`, defaulting from the id), current version.
  - `launch(id)`: boot the app's current version from the store.
  - `remove(id)`: full uninstall, as decided above.
  - Module registered in every build like `srt:dev` (static imports must
    resolve); the control (`go/control.rs install_apps_control`) only in
    go builds - elsewhere `available` is false, `list` is empty,
    launch/remove no-op. Typed in core's `runtime-modules.d.ts`.
  - `srt:apps` is launcher-intended, not launcher-enforced: any app in
    the go client could import it, same as `srt:dev`. A real privilege
    boundary is out of scope.
- Store growth in `go/store.rs`: `load(id)` generalizes `load_last()`
  (same `BootVersion` result); `remove_app(id)` deletes the app folder
  and clears the offline-boot pointer when it named that app; a listing
  helper walks `apps/` + reads each installed manifest, skipping
  anchor-only data sandboxes. Ids are validated strictly in the store
  (`app_dir`'s fall-back-to-"default" on bad ids must never redirect a
  delete or launch); `storage::apps_root()` exposes the apps level
  (None in the packed flat layout).
- Launch path reuses the existing boot machinery end to end:
  `EngineCmd::Reload { code, app_id }` + sandbox anchor + assets mount,
  i.e. what a dev push does today. Removing an app other than the
  running one is race-free in the obvious case (the launcher is what's
  running while managing). Known gap, shared with the dev-push install
  path: fonts register once at startup (Impeller's typography context is
  built with the engine's platform context and `register_font` is
  `&mut`), so a launched app's custom fonts apply from the next client
  start only. Mid-session font registration is its own backlog-sized
  piece of alloy work if it ever matters.
- Design process (agreed): written design direction first (short note,
  reacted to in prose), then 2-3 real `launcher.tsx` variants iterated
  in the live client via dev server + MCP snapshots; Claude Design /
  HTML mockups optional for early divergent exploration only, mood
  reference not spec. Mockups wanting renderer features we lack are a
  feature: each gap becomes an okf backlog candidate (grow the
  renderer), routed through the normal channel rather than blocking or
  silently constraining the launcher.
- Launcher UI, visually a clean break from the connect screen:
  - Installed-app list (tap to launch, delete with confirm) alongside
    the connect functionality.
  - The exploding-logo animation goes (`logo.tsx` deleted); the brand
    mark is the static puzzle icon, `lattice/assets/icon-puzzle.svg`
    rendered via the `<svg>` element (string inlined into the launcher
    bundle). Side effect worth having: an idle launcher stops running
    an `onFrame` animation, so it stops requesting frames.
  - Manual dev-server entry (text input for host:port; the pending
    item from the connection-strategy work). `srt:dev`'s
    `connect(addr)` already accepts an arbitrary address, so this is
    launcher UI only; entered addresses land in recents like any other
    connection.
  - QR scanning becomes its own screen instead of the inline
    fixed-width viewfinder: camera full screen (texture scaled to
    cover, center-cropped) with a scan marker drawn on top (the usual
    corner-bracket frame) and a cancel affordance. Same plumbing as
    today (`createCamera` with `scan: ["qr"]` mounted under a `<Show>`
    so the camera opens/closes with the screen; barcode result feeds
    `connect()`).

#### Reload/reconnect loop fix (2026-07-22, fell out of design iteration)

Dev-pushing any app that dials `connect(launchAddress)` at mount (the
launcher, and every launcher variant) used to loop forever: each mount
re-dialed, each new websocket got the server's latched push, each push
reloaded into another mount. The client showed rapid engine restarts
and never presented a frame. Fixed in `go/connection.rs run_direct`:
a connect naming the address (or ticket) the supervisor is already
dialing/serving is redundant and ignored instead of interrupting the
live connection. The JS-side `state() === "idle"` gate never worked
for this (the sticky state replay has not flushed at mount time) and
stays as-is; the native idempotence is the real guard.

#### Design direction (agreed with user 2026-07-22: use @solidrt/components, support dark AND light theme; launcher follows env.systemTheme)

The direction in one line: the launcher is a hallway, not a
destination - calm, static, and gone the moment an app opens.

- Hierarchy flip. Today's screen is 100% dev connection; the launcher
  inverts it. The installed-app list IS the screen; dev connection
  drops to a quiet secondary zone (a footer row / small affordance).
  Empty state flips back: no apps installed means the connect actions
  are the primary content, led by the puzzle mark and one plain line.
- Calm and static. No ambient motion, nothing pulsing; the only
  animation is feedback on interaction (press states, screen
  transitions). Idle launcher requests zero frames. Brand presence is
  the small static puzzle mark, not a hero graphic.
- Rows are text-first. Display name dominant, id + short version hash
  as one muted secondary line. Tap launches. Delete sits behind an
  explicit per-row affordance (not a swipe/long-press gesture for now)
  and confirms inline in the row - no modal layer.
- Three screens, one column. Home (list + connect zone), full-screen QR
  scan (camera cover-cropped, corner-bracket marker, cancel), and
  manual address entry (text field + recents). One shared spacing/type
  scale from the theme; no per-screen visual dialects.
- Dark neutral base, one accent. The launcher's surface recedes so app
  names carry the screen; the accent marks the primary action per
  screen (launch, scan, connect). App icons in rows are a future
  (needs manifest metadata), not designed around now.

### Stage 3: boot behavior flip (done 2026-07-22)

- The `load_last` startup block in ui_thread is deleted. Without an
  address: launcher. With an address: the launcher's existing
  `launchAddress`-driven connect covers dialing; the server's latched
  push boots the given app.
- Pruned everything the flip orphaned: `dev_auto_connect` plumbing and
  the native auto-connect special case in `go/session.rs` (the JS
  dial covers it, made safe by the redundant-connect fix),
  `merge_fonts` (no store boot at startup means no startup font merge;
  the mid-session font gap is now the only route for store-app custom
  fonts), `store::load_last`, and the whole `config.last_app` pointer
  (field, `save_last_app`, the install-path write, and remove_app's
  clearing) - written-never-read state once nothing boots from it.
- Verified against a live pair: no address = launcher offline; address
  with no latched entry = launcher stays up connected, zero store
  boots; address with latched entry = the push boots it. Mid-session
  `srt:apps launch()` verified end to end by a probe that launched a
  stored app (repeatedly - it launched itself). Full-screen scan
  screen verified with a real camera (cover-crop + brackets render
  over the live feed); QR decode is the same plumbing the old connect
  screen shipped with.
- The brand mark is the scaffold default template's gradient puzzle
  (7 d-path segments with per-segment linear gradients), rendered
  statically as `PuzzleMark`; the flat `icon-puzzle.svg` import is
  gone from the launcher.
- Scan reticle polish (2026-07-22, after camera testing): the overlay
  layers (texture, reticle, controls) are `position: "absolute"` - in
  flow they stacked in the column and sat off-center. Brackets are
  stroke 10 with `strokeCap/strokeJoin: "round"` and an arc at each
  bend (radius 20): the join-rounding alone (stroke/2) is
  imperceptible, the path arc is what reads as a rounded corner.
  Verified against a live camera; d-path arcs parse fine.

#### Variant chosen and promoted (2026-07-22)

Variant B ("cards"): centered puzzle mark (final size 144px), installed
apps as surface cards, dev connection as its own card with secondary
buttons. Iterated live via MCP snapshots; both themes verified (the
launcher follows `env.systemTheme`, dark until it resolves). Promoted
to `lattice/launcher/launcher.tsx`; `logo.tsx` and the exploration
variants deleted; bundle regenerated and the client rebuilt. Verified:
a manually started client (no address) boots the launcher offline with
the store's app list, one engine start, no errors.

## Master-detail iteration (done 2026-07-22)

The responsive future landed together with an app detail screen,
WhatsApp-style master-detail driven by the components policy layer
(`policy.layout`, no launcher-local breakpoints):

- Two-pane (`policy.layout === "twoPane"`, i.e. expanded width): left
  pane (380px) with the mark (96px), the app list and the dev card;
  right pane shows the selected app's details, or a muted "Select an
  app" empty state. Selected card highlighted (surfaceAlt).
- Single-pane: the home column as before (mark 144px); the detail is
  its own screen with a Back button. The same AppDetail component
  serves both.
- Cards are a single pressable each: pressing opens the detail view
  (no inline actions; an intermediate info icon-button variant was
  dropped by decision). Launch and Remove (confirm step) live in the
  detail view. The "Apps" heading and the scan screen's explanatory
  caption were removed.
- `srt:apps` grew `info(id)`: storage usage split into installed
  versions and data sandbox, the stored versions (current first,
  sizes; hardlink-shared assets count per version), and three file
  listings (claim vs truth, decided 2026-07-22): `assets` = the
  current version's manifest-declared entries, `files` = a recursive
  disk walk of the current version dir (bundle.js, manifest.json,
  assets as they really exist - divergence from the manifest is
  visible), `data` = a recursive disk walk of the sandbox (app-written
  files). Backed by `store::app_info` (`app_info_at` testable core
  with the same strict id validation as remove); DTO marshalled in
  plugins/apps.rs, typed in core runtime-modules.d.ts, covered by a
  store unit test. Sizes cross into JS as f64. The detail view shows
  Assets / Files / Data as separate cards.
- Verified live on the rebuilt release client: two-pane selection +
  detail (real sizes), forced single-pane home + detail via setPolicy
  probes, embedded launcher boot on a fresh pair. Deeper file-tree
  browsing deferred until a real need shows up.

## Futures (out of scope, recorded)

- Back-to-launcher gesture/key owned by the client (see
  okf/plans/exit-to-launcher.md).
- OTA + trust + factory seeding (storage stage 4); seeded apps then
  simply appear in the launcher's list.
- Deeper data-sandbox browsing (full folder tree) in the detail view;
  `info()` reports top-level entries today.
- If the dev-flow launcher flash (connect-to-push gap) ever bothers us,
  hold the first frame briefly; presentation tweak, not architecture.
