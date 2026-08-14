---
title: Update mechanism and client storage
description: "Survey and agreed direction: bundle OTA first with a signed manifest, dev and production converging on one client binary, a named data dir with a hardlinked per-app version store."
created: 2026-07-16
---

# Update mechanism and client storage

How installed solidrt apps get updates, and the on-disk storage/identity
model that fell out of thinking it through. Started as an updater survey;
ended as a design for the client data directory that unifies the dev pipe
and the production update pipe. Direction agreed 2026-07-16; not planned
or implemented yet.

## Survey

### How Discord does it

Discord's desktop client is Electron, but its update story is not stock
Electron, and it maps almost 1:1 onto our architecture. Two tiers:

- **Host**: the Electron shell plus a small bootstrap. Updated rarely,
  through a native updater. Historically Squirrel; since ~2019 replaced
  by Discord's own updater written in Rust, with delta (binary diff)
  updates. As of May 2026 that Rust updater also ships on Linux
  (previously Linux users had to reinstall manually).
- **Modules**: the actual application, split into named modules
  (`discord_desktop_core` carries the app JS; others: voice, spellcheck,
  utils, ...). At startup the bootstrap's `moduleUpdater` polls
  `https://discord.com/api/modules/<channel>/versions.json?platform=...&host_version=...`,
  downloads changed modules as zips into a pending directory, unpacks
  into versioned directories, swaps on restart.

Key property: `host_version` is a compatibility contract. The server only
offers module versions that work with the installed host. Almost all
Discord updates are fast module (JS) swaps; host updates are the rare
slow path. Channels (stable/PTB/canary) are just different endpoints.

### Other mechanisms

Native / whole-app updaters:

- **Squirrel** (what Electron's `autoUpdater` wraps): silent background
  download into side-by-side versioned directories (`app-1.2.3/`), swap
  on restart. Versioned dirs dodge Windows' "cannot overwrite a running
  exe" problem.
- **electron-updater**: same idea + GitHub Releases / generic HTTP
  feeds, blockmap (zsync-style chunk hashes) differential downloads,
  signature checks.
- **Sparkle / WinSparkle**: classic Mac in-app updater; XML appcast +
  EdDSA-signed archives; check while running, install on quit.
- **Omaha / Google Update** (Chrome): background OS service, staged
  rollouts, aggressive deltas (courgette/zucchini/puffin). Client open
  source, server closed; heavyweight to operate.
- **Tauri updater plugin**: static JSON manifest per platform/arch,
  minisign signature verified against a key baked into the app,
  download + verify + replace. No server logic; any static host works.
  Closest modern relative to what we would build.
- **Stores / package managers** (Play Store, App Store, MS Store, apt,
  Flatpak): the platform owns the pipeline. On Android effectively
  mandatory for native code (W^X: an app cannot exec a downloaded
  binary; same constraint as flux:ffi open_bytes).

App-layer (OTA bundle) updaters:

- **Expo EAS Update / CodePush model**: JS bundle + assets versioned
  separately from the native binary. The binary advertises a
  `runtimeVersion`; the server only serves matching bundles, so a bundle
  never calls native surface that is not there. Check on launch,
  download in background, apply on restart, keep the previous bundle;
  a bundle that crashes before marking itself healthy is rolled back.

Cross-cutting concerns every serious mechanism handles: payload signing
and verification (never trust the transport alone), a compatibility
contract between layers, atomic install (versioned dirs, never in-place
writes), rollback / last-known-good, channels, staged rollout.

## Design

### Two tiers

We already have Discord's split: a native runtime (flux/lattice, rarely
changing per app) and a JS bundle (the app, changing often). Two tiers,
built in this order:

1. **Bundle OTA** (the solidrt Expo Updates) - most value, least
   platform pain, identical on every OS including Android. Runtime
   carries a `runtimeVersion` (the flux/core surface contract, same role
   as Expo's runtimeVersion and Discord's host_version). A static signed
   manifest (Tauri-style, any HTTP host) lists the latest bundle per
   channel + runtimeVersion. Client checks on launch, installs into the
   version store (below), swaps on restart, rolls back on unhealthy.
   Dogfooding: the updater can be flux script code (fetch + flux:file);
   signature verification is the one piece that likely wants native help.
2. **Native runtime updater** - later, per platform. Android/iOS:
   store-only (W^X leaves no choice). Desktop: Squirrel-style
   side-by-side install dirs + swap on restart, manifest on the same
   static host; delta patching deferred (binary is not Chrome-sized).
   Linux can additionally defer to distro packaging.

Bundles are pure data, so tier 1 never hits locked-executable problems;
only tier 2 needs Squirrel-style tricks.

### Dev and production converge

solidrt-go (dev client: bundles pushed over websocket/iroh) and a
production app (bundles pulled from a manifest) are the same mechanism
with different policy. The distinction moves from two products to two
policies in one product:

- **One binary.** A production app is the generic client pinned to one
  app; solidrt-go is the unpinned version of the same binary.
- **One install path, two transports.** Dev push and OTA pull both feed
  the same version store, swap, rollback, and retention code. Dev then
  exercises the updater's least-tested machinery constantly.
- **The boundary that must NOT blur: trust.** Dev clients accept
  unsigned bundles from a paired dev server (default-accept LAN); a
  production app only accepts bundles signed by its publisher key,
  matching its pinned app identity and runtimeVersion. Installs record
  their source; a pinned client refuses dev-pushed installs.
- **Startup dependency.** Production cold-starts offline from the
  installed bundle; update checks are strictly background. Dev clients
  degrade the same way: offline relaunch of the last-received bundle.

### Packaging: code blob yes, asset blob no

One blob for code is fine (single JS bundle; later optionally QuickJS
bytecode - an orthogonal axis). Assets stay separate files:

- Streaming/seeking: audio already feeds SDL via SeekableSource over
  file(); inlined assets cannot be seeked and detour through the JS
  heap. Native decode wants bytes from disk.
- Startup: megabytes of base64 in source is parse tax + 4/3 inflation.
- Update economics: separate content-addressed files mean an update
  downloads only what changed; one blob re-ships every asset for a
  one-line code fix.

Production bundle shape: `bundle.js` + `assets/` (hashed filenames) +
`manifest.json` (bundle, assets, runtimeVersion, signatures).

Archives (asar/zip) are **immutable transport/install wrappers only**,
never mutated storage: asar is a front JSON index + concatenated
contents, so replacing one file rewrites the archive - there is no
in-place patch story anywhere in the ecosystem (electron-updater
blockmaps delta the transfer, then rewrite the whole file; Discord
unpacks module zips and discards them). Single-file artifacts make
sense at the edges: desktop initial install, and the APK/IPA already
plays that role on mobile. Storage is always loose files.

How code references assets - DECIDED (2026-07-20, revising the earlier
imports-first lean): convention-first. A designated `assets/` folder in
the project (subfolders free-form, e.g. `assets/fonts/`) is collected
WHOLESALE into the version manifest - zero config, no bundler analysis,
no hidden magic; `solidrt.fonts` entries point into `assets/fonts/`.
Code references assets by path (`file("assets/sounds/boing.ogg")`),
resolved against the bundle base path at runtime (dev server URL in
dev, `versions/<v>/` in production) - the same base-path resolution the
client storage design already requires. Dynamic paths work by
construction. See okf/plans/client-storage-updates.md (Project layout).

Static imports are optional later sugar, not the mechanism:

- **`file` loader**: `import boing from "./boing.ogg"` - the bundler
  emits the asset as a separate hashed output file and the import
  compiles to a path string (`var boing = "assets/boing-3f2a.ogg"`).
  Only the string lands in the JS/bytecode. Static = imports are
  top-level literals, so the bundler can enumerate imported assets at
  build time: typed bindings (flux-types declares `*.ogg` module
  types) and dead-asset elimination. Bun.build supports this directly
  (`loader: { ".ogg": "file" }`).
- **`text` / `dataurl` loaders (opt-in inline)**: for assets small
  enough to be treated as code - SVG document strings (fits `<svg src>`
  as-is), GLSL shaders, tiny icons. Wins: zero file I/O, available
  synchronously before base-path resolution exists, can never be
  missing or version-skewed. Costs (why it stays opt-in and small):
  base64 inflates 4/3 and decodes into the JS heap for the module's
  lifetime; no streaming/seek, so audio/video are disqualified; loses
  per-asset update granularity. There is no rawer form than a string:
  JS has no binary literal, so a bytes-exporting loader is base64 +
  decode underneath. Vite-style size-threshold auto-inline is a
  possible later convenience. Note: wanting single-file distribution
  is a container problem (the archive wrapper), not a reason to
  inline - inlining is per-asset, not a mode.

The layering matches the house API philosophy: plain path references
against the assets/ convention are the primitive; imports are an
opt-in convenience layer.

### Data roots

Storage today goes through the SDL pref path. `SDL_GetPrefPath(org, app)`
per platform (org included when non-empty):

- Linux: `~/.local/share/<org>/<app>/` (`$XDG_DATA_HOME` if set)
- macOS: `~/Library/Application Support/<org>/<app>/`
- Windows: `%APPDATA%\<org>\<app>\` (roaming profile)
- Android: org/app IGNORED - the app's internal files dir
  (`/data/data/<package>/files/`); the sandbox is the namespace,
  keyed by the APK package id
- iOS: app sandbox `Library/Application Support/`; per-app by
  construction

Resolution rule (decided), one knob, no second code path:

1. explicit `--data-root` (dev, tests) - the dev server passes
   `--data-root <project>/.srt-data` when spawning clients;
2. pinned production app: pref path from the app manifest's org/app -
   an app lives under ITS OWN identity (e.g. `MyOrg/MyApp`),
   never under the runtime's namespace (Discord does not store under
   `~/.local/share/Electron/`);
3. generic client: pref path `SolidRT/go`. The name stays: after the
   production split it refers to exactly the thing that uses it (the
   unpinned Expo-Go-style client); rename only if that becomes a
   user-facing launcher product (a dev-cache migration, not user data).

Dev default `.srt-data/` in the project root (decided; gitignored,
precedent `.expo/` / `target/`): project self-contained, delete =
factory reset, test fleet lives next to the code, no cross-project
state. Composes with `srt record`/`playback` (deterministic runs pin a
fresh or fixture data root).

### Client storage model

A client on a machine = a data directory (browser-profile model,
Chrome's `--user-data-dir`). Clients have **stable, user-chosen names**
(decided): a disposable client cannot re-pair as the same device
tomorrow, and named clients enable scripted multi-device test setups.
A client holds **multiple apps** (decided) - the client is the outer
container, device-analogy taken seriously:

```
<data-root>/clients/<name>/
  identity/          iroh secret key, client metadata
  apps/<app-id>/
    versions/<v>/    bundle.js, assets/... (immutable)
    state.json       { current, previous, healthy, launch counter }
    data/            flux:sqlite dbs, file() writes (the app sandbox)
  cache/  logs/      client-level
```

Placement caveat for `cache/` (noted 2026-07-17, from the fetch-cache
work): on Android and iOS, purgeable content must live in the platform
cache directory (`getCacheDir()` / `Library/Caches`) so the OS can reclaim
it and "Clear cache" works - NOT under the pref path, which is files/backup
territory. So the data-root resolution should resolve `cache/` to the
platform cache dir on mobile instead of a subdir of the data root. On
Android that is `SDL_GetAndroidCachePath()` (SDL_system.h, alongside the
internal/external storage paths): SDL exposes it and sdl3-sys binds it,
only the safe sdl3 crate has no wrapper, so it is a one-line sdl_utils
call and NOT the JNI hop an earlier revision of this note assumed.
Caches are self-contained and refetchable, so the split loses nothing.

The tree is IDENTICAL under every data root (decided; REVISED
2026-07-21 after walking a real install: production roots flatten -
a packed app's pref path keyed by appId alone is the whole tree, the
generic client drops the `clients/` level - see the plan's Client
data section, which is now authoritative for the layout). `apps/<app-id>` is load-bearing in dev: a project
with many entry points (this repo's `examples/`) is many apps sharing
one client fleet - same identities and pairings, separate data,
versions, and health state per example.

- **Identity is the reason per-client dirs are mandatory**: two clients
  sharing an iroh key would be the same node id (dev server could not
  tell them apart; pinned ports bind once). The folder IS the identity.
  Payoff: multi-device testing on one machine (p2p pairing, future MLS
  device groups) - a client is indistinguishable from a separate device.
- **Version store**: install version N+1 by hardlinking every manifest
  entry whose hash matches a file in version N, downloading the rest.
  Content-addressed dedup and delta downloads without a separate blob
  store; the version dirs are the store. Pruning is trivial (hardlinks
  keep shared data alive; `rm -rf` an app = clean uninstall).
- **Current pointer is a state file, not a symlink**: symlinks need
  privileges/dev-mode on Windows; `state.json` swapped by atomic rename
  is portable and holds the rollback bookkeeping anyway. Runtime reads
  it at startup and resolves the bundle base path from it - the same
  base-path resolution production file() needs.
- **Health/rollback protocol**: on switching current, healthy=false +
  launch counter; app/runtime marks healthy (first good frame or
  explicit markHealthy()); N crashes before healthy -> revert to
  previous, quarantine the new version. (Expo / Android A/B pattern.)
- **Per-app data sandbox**: with multiple apps per client, file() and
  flux:sqlite resolve inside `apps/<app-id>/data/` - the app gets a
  rooted filesystem capability, not the client dir.
- **Version identity in dev = content hash**: dev builds have no
  numbers; a version is a manifest of hashes, production adds a human
  label. App identity needs a stable app-id across dev sessions
  (package.json name is the candidate; renames orphan an entry, which
  is acceptable cache behavior). Production needs the id anyway for
  manifests.
- **Retention differs by context**: production keeps current +
  last-known-good; dev (a bundle per save) prunes aggressively (keep
  last N, allow pinning named versions). Same mechanism, different knob.
- **Rejected: cross-app dedup** via a client-global content store.
  Marginal value, turns pruning into real GC.

### Manifests

Two files with opposite lifecycles; neither is hand-synced with the
other (the version manifest is derived output).

**Project config** (hand-written, source-side): a `solidrt` key in
package.json, not a separate file (decided; Electron precedent -
package.json already carries `name` and Bun already reads it).
Graduate to `srt.config.ts` only if it grows (signing config,
per-channel settings); starting there now is a file tax.

```json
{
  "name": "myapp",
  "solidrt": {
    "appId": "com.example.myapp",
    "displayName": "My App",
    "org": "My Org",
    "entry": "src/app.tsx",
    "icon": "icon.svg"
  }
}
```

`appId` as explicit reverse-DNS solves three problems at once: the
stable identity that survives renames, the Android package id the APK
needs anyway, and (with `org`/`displayName`) the source of the
SDL_GetPrefPath org/app for pinned apps. Everything else defaults from
`name`/convention so a dev project needs zero config; `appId` itself
defaults to `name` in dev with a warning at publish time. No file list
in the project config, ever - the asset set comes from the bundler.

**Version manifest** (generated per build by publish/dev-build, next to
the bundle):

```json
{
  "appId": "com.example.myapp",
  "runtimeVersion": "3",
  "label": "1.4.2",
  "createdAt": "2026-07-16T12:00:00Z",
  "bundle": { "path": "bundle.js", "hash": "sha256:ab12...", "size": 148213 },
  "assets": [
    { "path": "assets/boing-3f2ac81.ogg", "hash": "sha256:9c4f...", "size": 88320 }
  ]
}
```

plus a detached signature over the canonical manifest bytes
(minisign-style). Key properties:

- **The version id IS the hash of the manifest.** Unifies dev and
  production identity: dev versions are manifest hashes with no
  `label`; production adds the human label.
- **Transitive integrity**: verify one signature on the manifest, then
  every file verifies by its hash from the manifest (the TUF idea
  reduced to one level, which is as much as we need).
- **Install = manifest diff**: hardlink entries whose hashes match the
  previous version, download the rest.

**Update endpoint is a URL convention, not a format**: the client
fetches `https://<host>/<appId>/<channel>/<runtimeVersion>/manifest.json`
and compares its hash against the installed version. Publishing =
upload the version files + overwrite that one pointer. No server
logic, any static host. The dev-push path delivers the same manifest +
files over the socket; the installer cannot tell the difference except
the missing signature - exactly the trust flag above.

### Fonts (cross-ref, 2026-07-20)

Fonts are assets under this model with one twist: a native consumer at
startup plus role metadata. The version manifest gains a `fonts` array
(asset entries annotated with an optional register-alias). Fonts ship
FIRST, ahead of this work, via packed trailer sections with in-memory
registration; the migration to version-store files and the manifest
annotation are specified in okf/plans/packaged-fonts.md (Alignment
section). When dev push carries fonts as assets, custom fonts become
visible in dev and solidrt-go's embedded Notos degrade to the
no-app-loaded fallback.

## Open questions

- Static-import sugar over the assets/ convention: design when
  wanted (loader/threshold details).
- What defines `runtimeVersion`: manual bump, or derived from the
  flux-types surface (ties into flux-types parity / surface.json).
- Signing scheme: minisign/ed25519 like Tauri is simple and likely
  sufficient; TUF is the paranoid option, probably overkill.
- Publishing: `srt publish`(?) produces bundle + version manifest onto
  plain static hosting; command shape undecided.
- Launcher UI for multi-app clients (the storage supports it; whether
  to surface it is separate).
- Whether bundle downloads could also ride the iroh p2p path
  (device-to-device update forwarding) - fun, not stage 1.

Status: open. Agreed 2026-07-16: two tiers, dev/prod convergence with
trust boundary, stable client names, multi-app clients,
dev-push-as-install, per-app sandbox, content-hash versions, no
cross-app dedup, data-root resolution (.srt-data dev default, manifest
org/app for pinned apps, SolidRT/go kept), identical tree under every
root, package.json `solidrt` key + generated signed version manifest
(version id = manifest hash) + static URL update endpoint.
Implementation plan: okf/plans/client-storage-updates.md (drafted
2026-07-20).
