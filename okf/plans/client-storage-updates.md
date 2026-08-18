---
title: Client storage and bundle updates
description: "Implements the update-mechanism research: data-root resolution, a hardlinked version store with dev-push-as-install and offline relaunch, assets in the manifest, then signed OTA."
created: 2026-07-20
---

# Client storage and bundle updates

Implements the agreed design in `okf/notes/update-mechanism.md`
(authoritative for rationale and rejected alternatives; this plan only
records staging, integration points, and plan-level decisions). Scope:
data roots, the client/app storage tree, the version store, manifests,
dev-push-as-install, and OTA pull. Out of scope: tier 2 (native runtime
self-update), launcher UI, p2p update forwarding.

## Status quo (2026-07-20)

- `srt pack` produces a self-contained executable: prebuilt runner +
  sectioned trailer (bytecode + font sections), writer
  `packages/cli/src/packer.ts` (kinds: 1 = bytecode, 2 = font), reader
  `lattice/src/main.rs` (`load_embedded_payload`). fluxrt still uses the
  old single-payload trailer.
- All persistent storage goes to the SDL pref path hardcoded as
  `SolidRT/go`: cwd anchor (`lattice/src/lib.rs:165`), dev-client
  `config.json` recents (`lattice/src/go/config.rs`), fetch cache
  `<pref>/cache` (`lib.rs:387`, comment already flags it interim
  pending this plan). LATENT BUG: packed apps share this dir with the
  dev client and each other - no identity, no namespacing.
- Dev bundles are JS source strings over the WebSocket, held in memory
  only (server latch `packages/cli/server/rebuild.ts`; client
  `lattice/src/go/connection.rs` -> `EngineCmd::Reload`). Nothing is
  persisted client-side; a client is useless without a live server.
- Identity: dev server persists its tunnel key (`.srt-tunnel-key`,
  project-local); the go client binds an EPHEMERAL key per run
  (`lattice/src/go/tunnel.rs:40`, dial-only today).

## End state

A packed app ships with a "factory version": the version payload it was
built with, either as a flat folder next to the runner (folder
distribution) or embedded in the trailer (single-file distribution),
one reader behind both. It boots from the factory version when the
version store is empty; installed versions take precedence -
Android-system-image/Expo-embedded-bundle semantics. The dev client and
a packed app run the same storage code; only the data root and trust
policy differ.

## Project layout

Convention-first: everything under `assets/` ships.

```
project/
  package.json          solidrt key (appId, org, fonts, ...)
  src/app.tsx
  assets/
    fonts/Inter.ttf
    sounds/boing.ogg
    icon.svg
```

The `assets/` tree is collected wholesale into the version manifest
(hashed output names), no bundler analysis - zero config, no hidden
magic. `solidrt.fonts` entries point into `assets/fonts/`. Static
imports (`import boing from "./boing.ogg"` -> typed path string +
dead-asset elimination) are demoted to later optional sugar, not the
mechanism; inline loaders (`text`/`dataurl`) remain available for
small/text assets regardless.

## Client data

Everything a client persists lives under one data root. The tree depends
on how the process runs (layout revision 2026-07-21; stage 1 originally
used one identical tree under every root, which put a packed app six
levels deep under a doubled org/displayName pref path):

```
explicit --data-root (opt-in; was the dev default until 2026-07-24):
  <data-root>/
    http-cache.db           dev server proxy cache (server-side,
                            was ./.srt-cache.db; project-local .srt-data/,
                            not the client root, since 2026-07-24)
    client<N>/
      identity/             client identity (persisted iroh key)
      apps/
        <app-id>/
          data/             app sandbox: sqlite, file() writes; process cwd
          cache/            the app's caches (fetch disk cache, per-app)
          versions/<hash>/  installed bundles
          state.json        current/previous/healthy
      logs/
      config.json           go-client persisted state (recents)

packed app: the pref path under the shared SolidRT vendor level, keyed
by appId, IS the app's folder - an installed app has one client and one
app, so neither is a directory. Grouping under SolidRT (2026-07-21,
same-day amendment to the flat form: many small apps must not clutter
the platform data dir; Flatpak-style, and one SolidRT folder per
machine total). org/displayName are display metadata with no storage
meaning; appId keying is rename-proof (Flatpak, macOS bundle ids,
Android convention):
  ~/.local/share/SolidRT/<appId>/    %APPDATA%\SolidRT\<appId>\, ...
    identity/  data/  cache/  logs/    (+ versions/, state.json in stage 4)

generic go client: many numbered clients, many apps, same vendor level
(client level restored 2026-07-24; numbered same day, names dropped):
  <pref SolidRT/go>/client<N>/
    identity/  apps/<app-id>/{data,cache}/  logs/  config.json
```

`--client <N>` selects a tree under an explicit `--data-root` or the
generic go client (default 0). Numbers are user-chosen, never
auto-allocated: allocation would shuffle data and identity between runs
depending on start order. The dir name is `client<N>` under both roots -
the clients/ intermediate level is gone everywhere (the go pref dir
holds nothing but client trees, and under an explicit root the
predictable client<N> names cannot collide with server-side files). A
packed app has exactly one client, so there `--client` is warned and
ignored. No running-instance check or lock: multiple instances of one
client (or one packed app) may run concurrently; not overwriting its own
files is the app's responsibility. Caveat: instances of the same client
share identity/p2p.key, so concurrent tunnel binds fight over one iroh
identity - the answer is --client 1. `.srt-tunnel-key` stays at the
project root. An old `./.srt-cache.db` is stale and can be deleted.

Data is always local to the machine the client process runs on - the
dev server never stores client data. An explicit `--data-root` only
reaches clients the CLI spawns on the dev machine; a client on
another machine (remote desktop, Android) resolves its own local root
(`SolidRT/go` pref path / app sandbox). Stage 1 caveat: remote dev
clients do not yet know the project's app id (it travels with the dev
push in stage 2), so their sandbox is `apps/default` - cross-project
sharing on a remote client persists until stage 2, as today.

## Pack output

Canonical `srt pack` output is a flat folder per target:

```
dist/<target>/
  solidrt(.exe)         runner binary
  manifest.json         the version manifest
  bundle.bin            bytecode
  assets/...
```

No app/ subfolder: the manifest enumerates exactly the files belonging
to the version, so membership is unambiguous - the runner is simply not
listed. The folder IS a version dir plus a runner; installing = the
same manifest+files shape the store uses.

Per-target wrappers consume this folder: the current trailer
single-file exe becomes the first wrapper (the "one file" case); a
bare folder is already what Steam-style depots expect; APK/dmg/msi are
future wrappers with their own expectations.

## Stages

### Stage 1: data roots + storage tree (no updates yet)

The bare minimum that fixes the shared-pref-dir bug and gives every
later stage its ground.

- App identity: `solidrt.appId` (reverse-DNS) + `org` + `displayName`
  in the package.json `solidrt` key (extends the fonts key). `appId`
  defaults to `name` in dev; pack warns when defaulted.
- New trailer section kind 3 = app manifest (small JSON: appId, org,
  displayName) so a packed runtime knows who it is. Reader in
  `lattice/src/main.rs` alongside fonts.
- Data-root resolution in lattice: `--data-root` (or env var) >
  packed app's own org/app pref path > `SolidRT/go`. One function,
  used by everything below.
- Tree: `clients/<name>/` (name from `--client`, default `default`)
  with `identity/`, `apps/<app-id>/data/`, `cache/`, `logs/`.
- Migrate the three existing consumers: cwd anchor -> the app's
  `data/` sandbox dir; go-client `config.json` -> `clients/<name>/`;
  fetch cache -> `clients/<name>/cache/`.
- Dev server spawns clients with `--data-root <project>/.srt-data`;
  `.srt-data/` added to scaffold + repo gitignore.
- Migration policy for existing `SolidRT/go` contents: none (dev-only
  data today; recents and caches regenerate).

Verify: two packed apps + a dev client on one machine, disjoint
storage; two named dev clients, disjoint sqlite writes from the same
example app.

Stage 1 DONE 2026-07-20. Implementation notes:

- App-identity section is three length-prefixed UTF-8 strings, NOT the
  JSON the draft sketched: serde_json is a go-feature dependency and
  the packed runner should stay JSON-free outside the JS engine.
  Writer `packages/cli/src/packer.ts` (encodeIdentity), reader
  `lattice/src/storage.rs` (decode_app_identity). Old runners skip the
  unknown kind 3 gracefully.
- Identity config: `packages/cli/src/project.ts` (findProjectPackage +
  loadAppIdentity; fonts.ts now shares the package.json lookup).
  Explicit bad values fail the command; derived values are sanitized.
  `srt pack` prints the identity and warns when appId is defaulted.
- Resolution + tree: `lattice/src/storage.rs` (StorageSpec/resolve/
  OnceLock init+get), consumed by the cwd anchor and fetch cache in
  lib.rs and go/config.rs (config.json now at clients/<name>/).
  Unsafe client/app-id components (separators, dot-dots) fall back to
  "default" with a warning; relative --data-root is absolutized before
  the chdir.
- Flags: --data-root and --client on the runner (all builds);
  `clientStorageArgs()` in packages/cli/src/args.ts injects
  --data-root <cwd>/.srt-data for `srt run`/`srt client` spawns.
  Scaffold gitignore covers .srt-data/ (repo root already did).
- Verified: unit tests (decode + resolve + traversal rejection, 5 in
  lattice/src/tests/storage.rs); packed exe with explicit identity
  creates `<XDG>/Stage1Org/Stage One/clients/default/apps/
  com.example.stage1/data` and anchors cwd there; go client with
  --client alpha/beta creates disjoint trees under project .srt-data;
  end-to-end `srt run` spawns the client with the project data root
  (config.json reads confirmed from the client dir). Note for future
  debugging: `srt run` with stdin at EOF (non-interactive) races REPL
  shutdown against the client spawn - test it with a terminal.

### Stage 2: version store + dev-push-as-install

- Version manifest emitted by the bundle step (appId, runtimeVersion
  placeholder, bundle entry with sha256 + size; assets list empty for
  now - see decision below). Version id = hash of canonical manifest
  bytes.
- Store layout under `apps/<app-id>/`: `versions/<manifest-hash>/`
  (bundle.js + manifest.json) + `state.json` {current, previous,
  healthy, launches}, atomic temp-write-rename like config.rs does.
- Dev push becomes an install: the reload message carries the manifest
  + code; client writes the version dir, updates state.json, then
  applies the engine reload as today. Source recorded as `dev`
  (unsigned) in state.
- Offline relaunch: on boot, resolve state.json -> load current
  installed version; fall back to embedded trailer bytecode ("factory
  version"), then to connect-screen (go client).
- Retention: dev prunes to last N (start N=5) at install time,
  hardlink rule not needed until assets exist (single bundle file).
- Stable client identity: persist an iroh secret key in
  `clients/<name>/identity/` and use it for the go-client tunnel bind
  so the dev server's client list is stable across restarts.

Verify: kill dev server, relaunch client offline, app boots from
store; prune leaves N; `list_clients` node id stable across client
restarts.

Stage 2 DONE 2026-07-20. Implementation notes:

- Manifest: built by every bundle path (`buildManifest` in
  packages/cli/src/project.ts, attached via bundler.ts BundleResult):
  {appId, runtimeVersion: 1, bundle: {sha256, size}}. Canonical = the
  exact JSON string the CLI serialized; it travels verbatim as the
  reload message's `manifest` string field (never re-serialized), and
  the version id is the sha256 of those bytes. The BSOD trigger and
  bytecode one-shots carry no manifest and are never installed. Both
  producers emit it: srt (repl/watcher/initial latch) and the server's
  rebuild.ts via bundle-cli's {code, map, manifest}.
- Store: lattice/src/go/store.rs, go-only for now - packed apps
  receive no installs until OTA (stage 4), so the packed runner stays
  serde-free; lifting the gate re-opens the state.json format
  question. install verifies bundle hash + size against the manifest,
  stages versions/.tmp-<hash> then renames, writes state.json
  {version, current, previous, healthy, launches} atomically
  (config.rs pattern), prunes to 5 (current + previous always kept,
  mtime order), and a repush of the current version dedupes to a
  no-op. healthy/launches are written but unused until stage 4.
- Reload flow: connection.rs installs when a manifest is present (a
  failed install degrades to today's ephemeral push), records the app
  id as config.json `lastApp`, and sends EngineCmd::Reload {code,
  app_id}; lib.rs `anchor_app` re-anchors the cwd into
  apps/<id>/data. That closes the stage-1 remote-client apps/default
  caveat: the sandbox follows the pushed app everywhere.
- Offline relaunch: a go client launched WITH --dev-server boots
  lastApp's current version from the store immediately and DevSession
  itself issues the Connect (session-level; the JS launchAddress
  connect stays for the default-screen case) - the latched reload
  replaces the app when the server answers. Launched without an
  address the connect screen is unchanged, so Android icon-tap / QR
  pairing keeps its entry point. Listing installed apps on the
  connect screen is the eventual launcher story, out of scope here.
- Stable identity: tunnel.rs loads/persists
  clients/<name>/identity/p2p.key (64 hex chars, 0600 on unix) and
  binds the tunnel endpoint with it.
- Verified (debug builds): 6 new unit tests (install/state/prune/
  dedupe/hash-mismatch + key decode); end-to-end `srt run` installed
  the initial push (version dir name == sha256 of manifest.json,
  bundle hash matches its manifest entry), a watcher edit produced v2
  with previous tracking, reverting the edit deduped back into v1
  (two dirs, previous = v2), the MCP rebuild path attached the
  manifest, offline relaunch against a dead --dev-server booted the
  app from the store anchored at apps/<id>/data, and a no-address
  launch still shows the connect screen. Stable-node-id-across-
  restarts not yet exercised live (needs a tunnel session; covered by
  the key roundtrip test).

### Stage 3: assets in the store + pack output folder

Split at implementation time (2026-07-20): 3a = assets end-to-end in
dev, 3b = pack folder output, 3c = trailer exe as a wrapper over the
folder. All three implemented.

- Asset collection: the `assets/` tree lands in the version manifest,
  collected wholesale by the bundle step (see Project layout; no
  static-import pipeline in this stage).
- Runtime base-path resolution: asset paths resolve against the
  current version dir (production/offline) or dev-server URL (live
  dev), the same resolution `file()` uses.
- Install applies the manifest-diff hardlink rule (link matching
  hashes from previous version, fetch the rest).
- Fonts: migrate packaged-fonts' interim in-memory registration to
  manifest-annotated assets in the version store (per the
  packaged-fonts plan's forward pointer); factory fonts remain the
  fallback.
- `srt pack` gains the canonical flat folder output (see Pack
  output); the trailer exe is reimplemented as a wrapper over it.
  Factory-version reader handles both adjacent-folder and trailer.
  (3b/3c; re-opens the packed runner's serde-free question for
  reading manifest.json natively.)

Stage 3b DONE 2026-07-20. Implementation notes:

- serde decision (user-approved): serde/serde_json are regular
  lattice deps in EVERY build now - the packed runner reads
  manifest.json natively, and stage 4 OTA needs it in packed apps
  anyway. Size delta not measured (user waived). forge is a regular
  dep too (assets mount setter). The manifest types moved out of
  go/store.rs into shared lattice/src/manifest.rs (parse/load/
  load_fonts + safe_asset_path); the store (installs) stays go-only
  until stage 4. The trailer identity section keeps its length-
  prefixed encoding (pinned CLI/runner pair, no reason to churn).
- Pack manifests (pack-folder.ts buildPackFolder): add top-level
  org + displayName (the folder has no trailer to carry identity;
  dev manifests still omit them, so dev version ids did not churn
  from 3a), bundle = {path: "bundle.bin", sha256, size} over the
  bytecode, assets = collected tree PLUS default fonts materialized
  under assets/fonts/<Noto file> (a user file already at such a path
  must be byte-identical or pack fails), fonts = the FULL resolved
  set in role order. fonts.ts split into resolvePackFonts (paths +
  isDefault) and loadPackFonts (bytes, trailer path). RUNTIME_VERSION
  const shared by both manifest builders (project.ts).
- `srt pack --folder [-o dir]` (default dist/) writes runner copy
  (dereferenced!) + manifest.json + bundle.bin + assets/. An
  existing non-empty output dir is only reused when it already
  holds a manifest.json (then the owned files are replaced); other
  dirs are refused. Single-file exe remains the default output and
  its trailer is untouched until 3c.
- Runner boot precedence (main.rs, non-go): embedded trailer >
  explicit path argument > exe-adjacent manifest.json
  (load_adjacent_folder). Folder boot: bundle path must be a plain
  filename; a .js bundle evals as source, anything else as QuickJS
  bytecode (so a hand-rolled folder from a dev manifest works);
  fonts register from the manifest annotations (the runner is
  font-free, no merge logic); identity comes from the manifest
  (missing org/displayName default from appId); the assets mount
  points at the folder. No per-boot hash verification - same trust
  as the trailer; signing is stage 4.
- Scaffold fold-in: `srt init` creates assets/ up front (the
  watcher only picks up a folder that exists at start), scaffold
  gitignore gains dist/, AGENTS.md gains an "Assets and app
  identity" section (assets/ convention, inline-import tradeoff,
  solidrt.fonts under assets/fonts/, appId guidance, pack forms).
  appId deliberately NOT pre-filled: the name-derived default plus
  the pack warning is the nudge toward a real reverse-DNS id.
- Verified (debug builds): both feature sets compile, 14 lattice
  tests pass; `srt pack --folder` on the 3a scratch project wrote
  the folder (custom sans + materialized serif/mono defaults, no
  NotoSans since the role was replaced; identity + bundle.bin in
  the manifest); the folder runner launched from an unrelated cwd
  anchored under its own pref-path identity and read assets via the
  mount; the default single-file pack still boots from its trailer
  (assets absent there as expected until 3c).

Stage 3c DONE 2026-07-20. Implementation notes:

- Trailer format v2 (clean break, user-approved - nothing deployed
  needs compat): the solidrt trailer is now the pack folder in
  section form. Kinds: 1 = the canonical manifest JSON verbatim,
  2 = a manifest-listed file named by its manifest path (bundle.bin
  + every asset). The old bytecode/font/identity kinds are DELETED
  from writer and reader - bundle, fonts, identity all come from
  the manifest; storage.rs decode_app_identity and packer
  encodeIdentity are gone with them. Table entry name length
  widened u8 -> u16 (asset paths can exceed 255 bytes). Magic and
  tail layout unchanged; a stale SRT_HOME runner meeting a v2
  trailer degrades to "no payload" via the existing bounds checks.
  fluxrt's single-payload trailer untouched (packer packFlux).
- One factory reader (main.rs): trailer and adjacent folder both
  produce FactoryPayload {app, fonts, identity, AssetsBase};
  precedence trailer > path arg > folder. A .js bundle is source,
  anything else bytecode, both forms.
- Range reads: forge::fs AssetsBase is Dir(path) | Packed {exe,
  index: path -> (offset, len)}. Packed resolution serves read /
  read_range (bounds-exact) / stat (size, no mtime) / exists;
  read_dir + dir_exists synthesize listings from index prefixes;
  open_seekable returns a FileWindow (clamped Read+Seek over the
  exe file, File-like semantics: past-end seek ok, before-start
  errors) so audio streaming pulls ranges without unpacking.
  fs::open_seekable now returns SeekableReader directly (flux
  stops re-boxing).
- Pack pipeline: the default `srt pack` path reuses buildPackFolder
  and appends its manifest + files as sections (packer packSolid) -
  single-file is literally the wrapper over the always-built
  canonical folder content. loadPackFonts deleted
  (resolvePackFonts is the single source).
- Store seeding from the factory payload remains stage 4, with OTA.
- Verified (debug builds): forge 28 tests (2 new FileWindow tests),
  lattice 12 (identity codec tests removed with the codec), both
  feature sets compile; single-file pack of the scratch project ran
  from an unrelated cwd - whole read, stat size, and ranged read
  all served from inside the exe, anchored under its pref-path
  identity; an independent Python parser walked the trailer table
  and verified every file section's sha256 + size against the
  embedded manifest; folder pack and dev push (install + fetch +
  mount) regressions pass.

Stage 3a DONE 2026-07-20. Implementation notes:

- Manifest shape (canonical bytes changed, version ids roll): bundle
  gains `path` ("bundle.js"; pack's other names refused by the store
  until stage 4), plus `assets` (path/sha256/size, sorted, dotfiles
  skipped) and `fonts` (annotation-only {path, alias} pointing into
  the assets list, no duplicated hash). Empty arrays are omitted.
  Collection = collectAssets in packages/cli/src/project.ts, rooted
  at the project package.json dir (projectDirFor); `solidrt.fonts`
  path entries must live under assets/ or the bundle fails
  (throw-in-dev); `false` entries only drop pack defaults and have
  no manifest presence. Dev manifests annotate CUSTOM fonts only -
  the go client's embedded Notos stay the fallback for unbound
  roles; pack (3b) materializes the full resolved set instead.
- Dev server: /assets/* route serves the project dir's assets tree
  (new projectDir in config/state, moves with repl `load` like
  sourceDir; reuses the file route handler, so range reads work).
  The watcher also rebuilds on assets/ changes (second watcher when
  the project dir is not the entry dir; an assets/ folder created
  after startup needs a restart or `load`).
- Client install (store.rs): missing_assets(manifest) lists entries
  not held by current/previous (by path + manifest hash);
  connection.rs fetches those from the server's /assets/ route
  (reqwest, 30s timeout, inline in the connection task) and
  install(manifest, code, fetched) verifies every file against its
  manifest entry, hardlinks held assets (copy fallback), stages,
  renames. Unsafe manifest paths (escapes, backslashes, non-assets/
  roots) refuse the install. Any failure still degrades to the
  ephemeral push.
- Runtime resolution: a single assets mount in forge::fs
  (set_assets_base): relative paths under assets/ resolve into the
  mounted version dir, writes under assets/ error while mounted;
  file()/dir()/seekable all inherit it. lattice points it at the
  app's current version dir on every named reload (mount_assets) and
  at boot-from-store; --proxy-files continues to override the whole
  flux:fs module, and plain flux scripts are unaffected. Documented
  in flux-types fs.d.ts (the runtime page is docs/50-runtime/index.md).
- Fonts from the store: boot-from-store loads the manifest's font
  annotations (BootVersion.fonts) and merges them over the embedded
  defaults, REPLACING a role alias a custom font claims (Impeller
  merges same-alias registrations into one family). Registration
  stays startup-only: a newly pushed font shows after a client
  restart, which the store boot makes cheap. Live post-start
  registration is an alloy investigation, deliberately out of 3a.
- Verified (debug builds): 14 lattice unit tests (3 new: fetched
  asset verify/refuse, hardlink reuse incl. same-inode assert +
  changed-hash refetch refusal, unsafe path refusal); end-to-end
  with a scratch project (custom sans font + 2 assets): initial push
  fetched 3 assets, version dir holds manifest + bundle + assets
  tree, fonts annotation present; asset edit produced a new version
  with the two unchanged files hardlinked (same inode) and the
  running app read the new bytes immediately (mount moved with the
  reload); offline relaunch against a dead server booted from the
  store, re-anchored, logged "Registering 1 font(s) from the version
  store", and read the asset through the mount.

### Stage 4: OTA pull + trust

- Update check: fetch
  `https://<host>/<appId>/<channel>/<runtimeVersion>/manifest.json`,
  compare manifest hash with installed, background install, swap on
  restart. Updater logic as flux script where practical.
- Signing: detached signature over canonical manifest bytes; verify
  against publisher key embedded in the packed app (new trailer
  section or manifest field). Pinned apps refuse unsigned/dev
  installs; the dev client accepts both.
- Health/rollback: healthy=false + launch counter on swap; runtime
  marks healthy at first successful frame (explicit markHealthy() can
  come later); N crashes before healthy -> revert to previous,
  quarantine.
- `srt publish`: build + manifest + sign + upload-ready output dir
  (actual upload left to the user's static host tooling initially).

## Plan-level decisions

- Factory version = the packed payload (folder or trailer), store
  takes precedence (end state above). Single-file distribution stays
  available as the trailer wrapper.
- Project layout: `assets/` convention is the primary asset
  mechanism; static imports demoted to optional later sugar (revises
  the research note's earlier imports-first direction).
- Pack output: flat dist folder, no app/ subfolder - the manifest
  defines version membership, the runner is simply unlisted.
- Single-file exe stays the DEFAULT `srt pack` output (the trailer
  wrapper runs over the always-built canonical folder unless a
  folder target is selected). Trailer assets keep range-read
  semantics (known section offsets in a disk file), so streaming
  works from the embedded form too. Per-channel policy knob planned
  for disabling self-OTA where the channel owns updates (Steam).
- Manifest-before-assets: stage 2 ships a manifest with only the
  bundle entry so store mechanics land without waiting for the
  asset collection (stage 3).
- runtimeVersion: field present from stage 2, value manually bumped
  constant `1` until the derivation question is settled (candidate:
  derive from flux-types surface; revisit at stage 4 latest).
- Signing scheme: minisign/ed25519 (settled here unless review
  objects; TUF rejected as overkill in the research note).
- Layout revision (2026-07-21, after user walkthrough of a real
  install): packed apps flatten to a single pref-path folder keyed by
  appId (see Client data); grouped under the SolidRT vendor level the
  same day (chosen over a personal env override or an install-time
  folder chooser - a chooser needs a persisted pointer the runner can
  find at every launch, i.e. ambient config anyway, and the many-
  small-apps clutter case is everyone's, not one user's). AppIdentity
  is gone from the runner - StorageSpec carries only an optional app
  id; a packed app launched with --data-root gets the dev shape. The
  proxy cache moved from ./.srt-cache.db to .srt-data/http-cache.db
  (dev-server config gained keyDir for the tunnel key, which stays at
  the project root).
- Project-local dev default reverted (2026-07-24): the CLI no longer
  injects `--data-root <project>/.srt-data` when spawning clients -
  running from arbitrary folders scattered `.srt-data` trees across
  the filesystem. Dev clients now fall back to the `SolidRT/go` pref
  path like any other go client; `--data-root` remains as an explicit
  opt-in. Because dev now lives in the pref tree, the 2026-07-21
  "one client per device" flattening of the go layout no longer holds:
  the client level is restored there. Same day, names were dropped for
  numbers: `--client <N>` (u32, default 0), dir `client<N>/`, under both
  the go pref tree and an explicit root - the clients/ intermediate
  level is gone everywhere (see the layout section). Numbered clients
  are for concurrent instances, not personas, so a documenting name buys
  nothing; a planned running-instance lock was dropped instead of
  postponed - multiple instances of one client/app are allowed, file
  consistency is the app's responsibility. `--client` works without
  `--data-root`; only packed apps ignore it (one client by definition).
  Old flat `SolidRT/go/apps/*`, named `SolidRT/go/<name>/` and
  `clients/<name>/` trees are stale, no migration. The dev server's
  project-local proxy cache (`.srt-data/http-cache.db`) is unchanged -
  it is server-side state living where the server runs. The flux bins
  likewise dropped the cwd `.srt-data/cache` fetch cache
  (`FluxEngineBuilder::dev_cache_dir` deleted): bare flux scripts now
  run without a disk store entirely - storage policy left the engine,
  and only embedders opt in via the plain `cache_dir` builder.
- --proxy-files sunset (2026-07-21): the assets pipeline covers its
  load-bearing uses (remote clients reading project assets, audio
  streaming via range reads), and a whole-module fs override now
  actively bypasses the storage semantics (sandbox anchor, assets
  mount). Removed: the flag, the reload-message proxyFiles field, the
  ProxyFsModule/streaming-reader half of go/proxy.rs (http fetch proxy
  stays), and the server's PUT write + dir listing + X-SRT-Type (GET
  with range support stays for /assets and bundle serving). The one
  lost niche - transparent reads of dev-machine files outside assets/
  - gets an explicit tool if it ever comes back.

## Open

- `srt publish` command shape and host-side layout conventions.
- markHealthy() JS surface (defer until a real app needs richer
  health than first-frame).
- Whether fluxrt adopts the sectioned trailer (needed if flux scripts
  ever want packed assets; not blocking).
- Static imports as optional sugar over the assets/ convention
  (typed paths, dead-asset elimination); design when wanted.
- Additional pack wrappers (APK, dmg, msi, Steam depot specifics)
  as targets become real.

Status: in progress. Stages 1 + 2 + 3 (a/b/c) done + verified
2026-07-20 (debug builds; user run pending). Next: stage 4 (OTA
pull + minisign trust + health/rollback + srt publish; includes
factory-payload store seeding).
