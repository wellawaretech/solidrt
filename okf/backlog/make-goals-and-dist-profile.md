---
title: Make the build goals mean what they say
description: The root make all builds only lattice, dist has three OS-suffixed names for one host goal, and the publish path ships half its binaries unstripped; fix the goal names and split the dev profile from the publish profile.
created: 2026-08-26
---

# Make the build goals mean what they say

## Symptom

`make all` from the repo root does not build all of it. It forwards to
lattice, whose `all: client runtime` is exactly the two lattice binaries. The
flux binaries (`flux`, `fluxc`, `fluxrt`) - which are shipped in every
platform package - are not built, and there is no root goal that builds them
at all. Android is absent too, which is correct, but nothing says so.

The root Makefile is a one-line forwarder: every goal is
`$(MAKE) -C lattice $@`. That made sense when lattice produced every
buildable output and owned the per-OS include machinery (`Makefile.linux` /
`.darwin` / `.windows`, plus `PROFILE`, `SPEECH`, `VERSION`, `DIST`). It has
outlived that:

- **`dist` is not a lattice concern.** It builds flux binaries and populates
  an npm platform package. It lives in lattice only because the per-OS
  makefiles do.
- **`download-fonts` writes into `alloy/assets/fonts`.** It is in lattice by
  accident.
- **`clean` is misleading.** It removes `lattice/target` and the shared
  `dist/`, but flux, alloy and forge build into the workspace-root `target/`,
  which nothing in `make clean` touches.
- **`help` is lattice's help**, so any root-level goal has to be advertised
  from inside lattice's help text.

Separately, `dist-linux` / `dist-darwin` / `dist-windows` are three names for
one goal. Only one is ever defined - `lattice/Makefile` includes exactly one
per-OS makefile - so the OS suffix promises a choice that does not exist, and
`make dist-darwin` on Linux fails with "no rule to make target" rather than
anything useful. The genuine cross build is `dist-android`.

## Shape

Two independent pieces. Neither is large; a full prototype of the first was
written and reverted on 2026-08-26, so the mechanics below are verified
rather than guessed.

### 1. Goal names

- lattice's `all: client runtime` becomes `lattice: client runtime`.
- The root owns `lattice` (`-C lattice lattice`), `flux` (`-C flux build`),
  and a real `all: lattice flux`. Host-native only; Android and packaging
  stay explicit.
- `dist-linux` / `dist-darwin` / `dist-windows` collapse to `dist` in the
  three per-OS makefiles, and the four `make dist-*` lines in
  `.github/workflows/release.yml` follow.

Traps found while prototyping:

- **The speech kill-switch keys off the goal name.** `lattice/Makefile` has
  `ifneq ($(filter dist-%,$(MAKECMDGOALS)),)` -> `DIST=1` -> `override
  SPEECH = 0`. `dist` does not match `dist-%`, so renaming without touching
  the filter would silently let `make dist SPEECH=1` publish a build with
  Whisper linked in. Spell the goals out (`dist dist-android
  dist-android-armeabi-v7a`), which also drops the pre-existing quirk that
  `dist-clean` sets `DIST=1`.
- **`.PHONY` stops being optional.** `lattice`, `flux` and `dist` are all
  directory names at the repo root.
- **flux's collective goal must stay `build`.** `flux` is taken by the single
  binary, and `make build` is documented in `flux/README.md` and several
  `flux/examples/*.js` headers.

The deeper fix - moving `dist`, `clean`, `dist-clean`, `download-fonts` and
`help` up to the root and leaving lattice with `client`, `runtime` and the
Android goals - is the honest end state, but `dist` is per-OS, so either the
`Makefile.<os>` include moves up too or `dist` stays split. Not required for
the rename; decide separately.

### 2. Dev profile vs publish profile

`dist` today builds `solidrt` and `fluxrt` at `release-opt` and
`solidrt-go`, `flux`, `fluxc` at plain `release`. That split is not a
per-binary decision, it is two audiences tangled into one goal: `dist` is
both "populate `packages/` so I can test a packaged install" and "produce
what npm publishes".

`release-opt` is `release` + `lto = "fat"` + `codegen-units = 1` +
`strip = true` (root `Cargo.toml`). Plain `release` is the same optimization
level with symbols kept - it is the debuggable build, not a slower one, and
release-level optimization is non-negotiable either way because QuickJS is
unusable without it.

Make it one knob rather than three hardcoded choices:

```make
DIST_PROFILE ?= release-opt

dist:
	$(MAKE) client PROFILE=$(DIST_PROFILE)
	$(MAKE) -C $(SRT_HOME)/flux build PROFILE=$(DIST_PROFILE)
	$(MAKE) runtime PROFILE=$(DIST_PROFILE)
	cp ...
```

Publishing gets a uniformly optimized, stripped set; `make dist
DIST_PROFILE=release` gets today's debuggable one at today's speed. It also
collapses the flux section from two lines to one (`make -C flux build
PROFILE=...` already covers all three binaries, making `build-opt`
redundant on this path), and because each profile has its own cargo dir, a
dev alternating `make client` and `make dist` does not thrash a shared
target dir.

Open questions, both because nobody has ever built these that way:

- **Client build time under fat LTO.** `solidrt-go` is the big one (SDL +
  Impeller + QuickJS + the runtime) and the cost lands in all four release
  jobs. Measure `make client PROFILE=release-opt` first. If it is bad, the
  answer may be release-opt for `solidrt`/`fluxrt`/`flux` and plain `release`
  for the client - as a commented decision rather than an accident.
- **Stripping the client.** Binary size matters for the platform packages, so
  stripping is probably right, but a field crash from a published client then
  yields no symbols.
- `dist-android` builds the client `.so` at plain `release` and is arguably
  where stripping pays most. Same question, times three ABIs of CI time.

## Also here

- **`make dist` leaves untracked binaries in the git tree.** None of the
  per-package `.gitignore` files under `packages/<platform>/` are tracked
  (`git ls-files packages/ | grep gitignore` returns only the scaffold's), so
  on a fresh clone nothing ignores the five staged binaries and a `make dist`
  followed by `git add -A` commits them. The local copies are also
  inconsistent where they do exist: `linux-x64-gnu` and `darwin-arm64` have
  only `solidrt*`, `win32-x64-msvc` has `solidrt*.exe` and `*.dll` but misses
  `flux*.exe`, and only `linux-arm64-gnu` carries a `flux*` line. Commit one
  correct `.gitignore` per platform package; independent of everything above.
- **`run-android-armeabi-v7a` could be `run-android
  ANDROID_ABI=armeabi-v7a`.** The target is already just a forwarder, and
  `ANDROID_ABI` is the documented knob alongside `PROFILE=` and `SPEECH=`.
  `dist-android-armeabi-v7a` is a real second target (a 32-bit-only APK,
  since `ANDROID_ABIS` excludes v7a) and keeps its own name.
