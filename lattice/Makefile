# lattice/Makefile

VERSION         ?= $(shell git describe --tags --always --dirty | sed 's/^v//')
SOLIDRT_VERSION  = SOLIDRT_VERSION="$(VERSION)"

PROFILE ?= release
ifeq ($(PROFILE),release)
  CARGO_PROFILE_FLAG = --release
else ifeq ($(PROFILE),debug)
  CARGO_PROFILE_FLAG =
else
  CARGO_PROFILE_FLAG = --profile $(PROFILE)
endif

# Dist (production) builds are always speech-free. This is the single source
# of truth: any dist-* goal sets DIST=1, exports it to the recursive client/
# runtime makes, and the override beats even a command-line SPEECH=1.
ifneq ($(filter dist-%,$(MAKECMDGOALS)),)
DIST = 1
endif
export DIST
ifeq ($(DIST),1)
override SPEECH = 0
endif

# Speech recognition (Whisper + wake word) is an opt-in cargo feature (not in
# lattice's `default`), so a plain `cargo build` stays voice-free. The same
# holds for every Makefile target: speech is off unless explicitly requested
# with SPEECH=1 (no whisper-rs / livekit-wakeword / ort-tract otherwise), and
# dist-* builds force it off regardless (see DIST above). GO_FEATURES /
# RUNTIME_FEATURES are the resulting cargo feature flags for the two binaries
# (go = client, which already pulls flux/compile; runtime = default minus go).
#
# Note SPEECH=1 is impractical on 32-bit Android ABIs (armeabi-v7a, x86):
# whisper.cpp has no reliable NEON fp16 path there and the address space is
# too tight for model + inference buffers.
SPEECH ?= 0
ifeq ($(SPEECH),0)
GO_FEATURES      = --no-default-features --features go
RUNTIME_FEATURES = --no-default-features --features flux/compile
else
GO_FEATURES      = --features go,speech
RUNTIME_FEATURES = --features speech
endif

SRT_HOME ?= $(abspath $(CURDIR)/..)

LAUNCHER_DIR = $(SRT_HOME)/apps/launcher
LAUNCHER_SRC = $(LAUNCHER_DIR)/launcher.tsx
LAUNCHER_JS  = $(LAUNCHER_DIR)/launcher.srt.js

# The BSOD screen shown when an app fails to start.
BSOD_SRC = $(LAUNCHER_DIR)/bsod.tsx
BSOD_JS  = $(LAUNCHER_DIR)/bsod.srt.js

include $(SRT_HOME)/platform.mk

include Makefile.android
ifeq ($(findstring apple-darwin,$(HOST_TARGET)),apple-darwin)
include Makefile.darwin
else ifeq ($(findstring windows,$(HOST_TARGET)),windows)
include Makefile.windows
else
include Makefile.linux
endif

.DEFAULT_GOAL := client

.PHONY: all help launcher-bundle dist-clean clean

# Build both host binaries (client + runtime).
all: client runtime

# The monorepo packages both bundles bake in. Without these prerequisites a
# core/components surface change leaves a stale bundle importing a runtime
# export that no longer exists, failing at module link on engine start.
BUNDLE_DEPS = $(wildcard $(SRT_HOME)/packages/core/src/*.ts*) $(wildcard $(SRT_HOME)/packages/components/src/*.ts*)

$(LAUNCHER_JS): $(wildcard $(LAUNCHER_DIR)/*.tsx) $(wildcard $(LAUNCHER_DIR)/parts/*.tsx) $(BUNDLE_DEPS)
	@echo "Bundling launcher (launcher.tsx -> launcher.srt.js)..."
	cd $(CURDIR)/.. && bunx srt bundle $(LAUNCHER_SRC)

$(BSOD_JS): $(BSOD_SRC) $(BUNDLE_DEPS)
	@echo "Bundling BSOD screen (bsod.tsx -> bsod.srt.js)..."
	cd $(CURDIR)/.. && bunx srt bundle $(BSOD_SRC)

help:
	@echo "Host ($(HOST_TARGET)):"
	@echo "  client               Build the client (default)"
	@echo "  runtime              Build production runtime"
	@echo "  all                  Build both client and runtime"
	@echo "  dist-linux           Build both into packages/$(HOST_PLATFORM)/"
	@echo ""
	@echo "Android dev (single ABI=$(ANDROID_ABI)):"
	@echo "  client-android       Build the client .so"
	@echo "  run-android          Build, install, launch, tail logcat"
	@echo "  run-android-armeabi-v7a  Same, for the 32-bit ABI"
	@echo ""
	@echo "  The launched client auto-connects to DEV_SERVER=$(DEV_SERVER)."
	@echo "  Override it (10.0.2.2:$(DEV_PORT) for an emulator), or DEV_SERVER= for none."
	@echo ""
	@echo "Android dist (fat: all ABIs):"
	@echo "  dist-android         Build fat APK into packages/android-arm64-v8a/"
	@echo "  dist-android-armeabi-v7a  Build 32-bit APK into packages/android-armeabi-v7a/"
	@echo ""
	@echo "Pass PROFILE=debug for a debug build (default: release)."
	@echo "Pass SPEECH=1 to build with speech recognition (default: off)."
	@echo "dist-* targets always build speech-free; solidrt and fluxrt use release-opt."
	@echo ""
	@echo "Launcher:"
	@echo "  launcher-bundle      Regenerate launcher.srt.js from launcher.tsx"
	@echo ""
	@echo "Clean:"
	@echo "  clean                Remove all build artifacts"
	@echo "  dist-clean           Remove dist/ only"

# -- Launcher -----------------------------------------
#
# SolidRT Go embeds apps/launcher/launcher.srt.js at compile time via include_str!.
# The file target rebuilds launcher.srt.js when any .tsx source changes.
# Run `make launcher-bundle` to force a rebuild.

launcher-bundle: $(LAUNCHER_JS) $(BSOD_JS)

# -- Fonts --------------------------------------------

# The three Noto role defaults (sans/serif/mono). The go client embeds them
# (lattice lib.rs embedded_fonts); the production runtime ships no fonts, and
# `srt pack` appends these as trailer sections by default (see
# okf/plans/packaged-fonts.md).
FONTS_DIR = $(CURDIR)/../alloy/assets/fonts

download-fonts:
	curl -L "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans/NotoSans%5Bwdth%2Cwght%5D.ttf" -o $(FONTS_DIR)/NotoSans.ttf
	curl -L "https://raw.githubusercontent.com/google/fonts/main/ofl/notoserif/NotoSerif%5Bwdth%2Cwght%5D.ttf" -o $(FONTS_DIR)/NotoSerif.ttf
	curl -L "https://raw.githubusercontent.com/google/fonts/main/ofl/notosansmono/NotoSansMono%5Bwdth%2Cwght%5D.ttf" -o $(FONTS_DIR)/NotoSansMono.ttf
	curl -L "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans/OFL.txt" -o $(FONTS_DIR)/OFL.txt

.PHONY: download-fonts

# -- Clean --------------------------------------------

dist-clean:
	rm -rf $(SRT_HOME)/dist

clean:
	rm -rf target $(SRT_HOME)/dist
	rm -rf android/app/build android/build android/.gradle
