---
title: Make the build goals mean what they say
description: The publish path ships half its binaries unstripped because dist hardcodes release for some and release-opt for others; make the publish profile one knob. Goal-name cleanup landed 2026-08-28.
created: 2026-08-26
---

# Make the build goals mean what they say

## Landed (2026-08-28)

The goal names now mean what they say: root `make all` is `lattice flux`
(both lattice binaries plus the three flux binaries; host-native only),
`lattice`'s collective goal is `lattice`, the OS-suffixed `dist-<os>` goals
collapsed to `dist` (release.yml follows), the speech kill-switch lists its
goals explicitly (`dist android-dist android-dist-armeabi-v7a`, so
`dist-clean` no longer sets `DIST=1`), root `clean` also runs flux's
`cargo clean` for the workspace-root `target/`, and the per-platform-package
`.gitignore` files are replaced by one block in the root `.gitignore`.

Not done, decided separately: moving `dist`, `download-fonts` and `help` up
to the root. `dist` is per-OS through lattice's `Makefile.<os>` include, so
that drags the include up too; not worth it until something else needs it.

## Open: dev profile vs publish profile

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
- `android-dist` builds the client `.so` at plain `release` and is arguably
  where stripping pays most. Same question, times three ABIs of CI time.

## Also here

- **`android-run-armeabi-v7a` could be `android-run
  ANDROID_ABI=armeabi-v7a`.** The target is already just a forwarder, and
  `ANDROID_ABI` is the documented knob alongside `PROFILE=` and `SPEECH=`.
  `android-dist-armeabi-v7a` is a real second target (a 32-bit-only APK,
  since `ANDROID_ABIS` excludes v7a) and keeps its own name.
