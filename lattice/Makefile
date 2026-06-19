# lattice/Makefile

VERSION         ?= $(shell git describe --tags --always)
SOLIDRT_VERSION  = SOLIDRT_VERSION="$(VERSION)"

PROFILE            ?= release
CARGO_PROFILE_FLAG  = $(if $(filter release,$(PROFILE)),--release,)

# Speech recognition (Whisper + wake word) ships in the default feature set.
# SPEECH=0 drops it (no whisper-rs / livekit-wakeword), for builds that do not
# want the voice stack -- e.g. the release flow for now. GO_FEATURES /
# RUNTIME_FEATURES are the resulting cargo feature flags for the two binaries
# (go = client, which already pulls flux/compile; runtime = default minus go).
SPEECH ?= 1
ifeq ($(SPEECH),0)
GO_FEATURES      = --no-default-features --features go
RUNTIME_FEATURES = --no-default-features --features flux/compile
else
GO_FEATURES      = --features go
RUNTIME_FEATURES =
endif

DEFAULT_APP_SRC = $(CURDIR)/default-app/app.tsx
DEFAULT_APP_JS  = $(CURDIR)/default-app/app.srt.js

SRT_HOME ?= $(abspath $(CURDIR)/..)

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu  = linux-x64-gnu
PLATFORM_aarch64-apple-darwin      = darwin-arm64
PLATFORM_x86_64-pc-windows-msvc   = win32-x64-msvc
HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))
HOST_DIST      = $(SRT_HOME)/dist/$(HOST_PLATFORM)

include Makefile.android
ifeq ($(findstring apple-darwin,$(HOST_TARGET)),apple-darwin)
include Makefile.darwin
else ifeq ($(findstring windows,$(HOST_TARGET)),windows)
include Makefile.windows
else
include Makefile.linux
endif

.DEFAULT_GOAL := client

.PHONY: all help default-app-bundle dist-clean clean

# Build both host binaries (client + runtime).
all: client runtime

$(DEFAULT_APP_JS): $(wildcard $(CURDIR)/default-app/*.tsx)
	@echo "Bundling default app (app.tsx -> app.srt.js)..."
	cd $(CURDIR)/.. && bunx srt bundle $(DEFAULT_APP_SRC)

help:
	@echo "Host ($(HOST_TARGET)):"
	@echo "  client               Build the client (default)"
	@echo "  runtime              Build production runtime"
	@echo "  all                  Build both client and runtime"
	@echo "  dist-linux           Build both into packages/linux-x64-gnu/"
	@echo ""
	@echo "Android dev (single ABI=$(ANDROID_ABI)):"
	@echo "  client-android       Build the client .so"
	@echo "  run-android          Build, install, launch, tail logcat"
	@echo ""
	@echo "Android dist (fat: all ABIs):"
	@echo "  dist-android         Build fat APK into packages/android-arm64-v8a/"
	@echo ""
	@echo "Pass PROFILE=debug for a debug build (default: release)."
	@echo "Pass SPEECH=0 to build without speech recognition (default: on)."
	@echo ""
	@echo "Default app:"
	@echo "  default-app-bundle   Regenerate app.srt.js from app.tsx"
	@echo ""
	@echo "Clean:"
	@echo "  clean                Remove all build artifacts"
	@echo "  dist-clean           Remove dist/ only"

# -- Default app --------------------------------------
#
# SolidRT Go embeds default-app/app.srt.js at compile time via include_str!.
# The file target rebuilds app.srt.js when any .tsx source changes.
# Run `make default-app-bundle` to force a rebuild.

default-app-bundle: $(DEFAULT_APP_JS)

# -- Fonts --------------------------------------------

# Fonts moved to alloy with the render tree (embedded by alloy's platform.rs).
FONTS_DIR = $(CURDIR)/../alloy/assets/fonts

download-fonts:
	curl -L "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans/NotoSans%5Bwdth%2Cwght%5D.ttf" -o $(FONTS_DIR)/NotoSans.ttf
	curl -L "https://raw.githubusercontent.com/google/fonts/main/ofl/notosansmono/NotoSansMono%5Bwdth%2Cwght%5D.ttf" -o $(FONTS_DIR)/NotoSansMono.ttf
	curl -L "https://raw.githubusercontent.com/google/fonts/main/ofl/notosans/OFL.txt" -o $(FONTS_DIR)/OFL.txt

.PHONY: download-fonts

# -- Clean --------------------------------------------

dist-clean:
	rm -rf $(SRT_HOME)/dist

clean:
	rm -rf target $(SRT_HOME)/dist
	rm -rf android/app/build android/build android/.gradle
