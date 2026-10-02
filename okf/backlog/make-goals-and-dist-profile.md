---
title: Make the build goals mean what they say
description: Goal-name cleanup landed 2026-08-28; the publish profile was decided 2026-09-28 (one PROFILE for every dist binary, release by default, release-opt for publishing, Android alike) and verified on linux-x64 and Windows; open is verifying the darwin and Android publish builds.
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

## Decided (2026-09-28), verification open

One knob, the existing `PROFILE`, for every binary of a goal, `dist` and
`android-dist` included: plain `make dist` builds all five at `release`
(fast, symbols kept), `make dist PROFILE=release-opt` builds all five fat LTO
and stripped, client, flux and fluxc too; the Android client and runner both
follow it (`ANDROID_RUNTIME_PROFILE` is gone). release.yml passes
`PROFILE=release-opt` to every desktop and Android dist build. The desktop
`dist` recipes build flux's three binaries through `make -C flux build`.

Measured on a 24-core x86_64 box (Ubuntu 22.04 WSL), `make dist
PROFILE=release-opt` from a warm dependency cache: client 4m59s, flux 3m13s,
fluxc 31s, fluxrt 2m05s, solidrt 4m30s, 944 s end to end. Sizes: client
71 MB (release, unstripped) -> 46 MB, flux 40 -> 20 MB, solidrt 38 MB,
fluxrt 20 MB, fluxc 1 MB; all stripped, glibc 2.35. CI's 4-core runners will
be slower; the fat-LTO link is mostly serial.

Windows verified the same day (24-core box, MSVC, warm cache): client 7m00s,
flux 4m35s, fluxc 42s, fluxrt 2m54s, solidrt 6m23s, 1341 s end to end;
solidrt-go.exe 46 MB, solidrt.exe 38 MB, flux.exe and fluxrt.exe 20 MB, all
start.

Still to verify before this moves to `done/`: a darwin publish build (fat LTO
needs the clang_rt link-arg the client and runtime recipes pass; flux and
fluxc have never been built fat LTO there), and the Android client `.so` at
release-opt through cargo-ndk (never built; first test is a release dry run
or an Android builder).

## Open: dev profile vs publish profile (the analysis that led here)

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
	$(MAKE) -C $(SOLIDRT_HOME)/flux build PROFILE=$(DIST_PROFILE)
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
