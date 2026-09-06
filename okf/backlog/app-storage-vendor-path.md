---
title: A shipped app stores its data under its own vendor, not under solidrt
description: Packed apps currently land in <pref>/SolidRT/<app-id>/, forcing the engine's name into an end user's filesystem - no shipped game files itself under unreal/. Proposed rule - <pref>/<org>/<app-id>/ when solidrt.org is declared, <pref>/<app-id>/ when it is not, never a forced vendor level - with the player taking the same rule as <pref>/solidrt-go/. The config key exists but is display metadata only: org reaches neither the manifest nor the pack payload, so the plumbing is the work. Independent of the dev-server session work, since dev clients move to ~/.srt/clients/.
created: 2026-08-13
---

# A shipped app stores its data under its own vendor, not under solidrt

An installed packed app currently resolves its storage through
`get_pref_path("SolidRT", app_id)` (`lattice/src/storage.rs`), so an end
user gets `~/.local/share/SolidRT/com.example.app/`. That files someone
else's product under the name of the engine that happened to build it.

The test that settles it: Unreal games do not all land in an `unreal/`
folder. How an app was built is an implementation detail, and it should not
be visible in the filesystem of someone who just installed a program.

## The rule

```
<pref>/<org>/<app-id>/     when solidrt.org is declared
<pref>/<app-id>/           when it is not
```

Never a forced `solidrt/` level. The vendor grouping is opt-in and belongs
to the app's publisher: a vendor with five apps gets one folder holding
five, a vendor with one app can skip it entirely.

The player takes the same rule rather than an exception, which is what
finally removes the odd `SolidRT/go/` split - the product is called
`solidrt-go`, so splitting its name across two directory levels was an
artifact of SDL's `get_pref_path(org, app)` signature, not a decision:

```
<pref>/solidrt-go/         the player: identity/, config.json, logs/, apps/<id>/...
```

With no shared vendor namespace left, the hazard of a packed app whose id
collides with `go` (or `dev`, or any other name we squat) disappears with
it.

## What exists

- `solidrt.org` is already in the project config schema as the publisher
  field (`packages/cli/src/project.ts`), alongside `appId` and
  `displayName`, and `checkField` already rejects path separators, empty
  values and over-long strings in it.
- App ids are path-safe by construction: `sanitizeAppId` for derived values,
  `APP_ID_PATTERN` for explicit ones, and `safe_component` on the Rust side
  as the last gate.
- `get_pref_path(org, app)` has exactly one real call site,
  `lattice/src/storage.rs`. The other `"SolidRT"` strings in the tree are
  display names (window title, log tag, site name) and are unrelated.

## What is missing

1. **`org` does not reach the client.** `buildManifest` emits `appId`,
   `displayName`, `runtimeVersion`, `solidrtVersion`, `icon`, `bundle`,
   `assets` and `fonts` - no `org`. The pack payload likewise carries only
   `app_id` (`lattice/src/main.rs`). Both need the field, and the manifest
   is canonical (its sha256 is the version id), so adding a field changes
   every version hash - a one-time reinstall, not a migration hazard.
2. **`StorageSpec` takes only an app id.** It needs the optional org, and
   the packed branch needs to build a one- or two-level path from it.
3. **Validation of `org` as a path component.** `checkField` covers the
   display use; a directory name needs the same treatment `safe_component`
   gives app ids, with the same fallback behaviour.

## Open

- **Directory name: app id or display name?** Today it is the app id, which
  is stable across releases and path-safe by construction
  (`com.example.app`). SDL's convention would suggest the display name
  (`Example App`), which reads better in a file manager but can change
  between releases and drags in spaces and unicode. Preference: keep the
  id, on the grounds that an author who wants a pretty folder can choose a
  pretty id. Same question applies to `org`, which is a display string
  today.
- **Migration.** This changes where an already-installed app looks for its
  data, so an app that has shipped would come up with an empty sandbox.
  Pre-1.0 with approximately no installed base, so the answer is probably
  "just change it", but it should be a decision rather than an oversight.
- **Does SDL's Android backend use org/app at all**, or does it return the
  app-private files dir regardless? If it ignores them, the whole question
  is desktop-only and Android is already correct. Unverified.

Related: `okf/plans/client-storage-updates.md` (the plan that established
the current layout and its Flatpak-style vendor grouping, which this
partially reverses - the grouping stays available, it just stops being
ours to impose), `okf/backlog/parallel-dev-servers.md` (dev clients move to
`~/.srt/clients/`, which is why that work does not depend on this one).
