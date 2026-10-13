# lattice/Makefile
#
# The two host binaries (solidrt-go, the client; solidrt, the runtime) and
# their Android .so files are built and staged by the stock program's own
# Makefile, runtime/Makefile, which includes packages/cli/runtime/runtime.mk:
# the same file a custom runtime includes from its node_modules, so the
# checkout and an app build a runtime one way. The goals here wrap that call
# with what is the checkout's: the player bundles the program embeds, the
# ANGLE libraries on Windows and macOS, the Gradle APKs, the flux binaries,
# the platform packages and their notices.
#
# The build flags are the runtime's (PROFILE, VIDEO, KTX2,
# WASM_NATIVE, TEST, ANDROID_ABI, NDK_HOME, documented by `make -C runtime
# help`); a value on this command line reaches it.

VERSION ?= $(shell git describe --tags --always --dirty | sed 's/^v//')

SOLIDRT_HOME ?= $(abspath $(CURDIR)/..)

# The runtime's build, staged into the checkout's dist/ (the SOLIDRT_HOME
# layout sol reads), reporting this checkout's version. GL_SRC= because the
# ANGLE libraries are staged here (angle-darwin, angle-windows), not taken
# from a platform package.
RUNTIME_MAKE = $(MAKE) -C $(SOLIDRT_HOME)/runtime DIST_DIR=$(SOLIDRT_HOME)/dist VERSION=$(VERSION) GL_SRC=

# The player app sources live in apps/player like any regular app; the
# generated bundles land in packages/cli/player/, where every runtime embeds
# them from (include_str! in runtime/src/lib.rs through node_modules) and
# where the CLI package ships them for a custom runtime.
PLAYER_DIR = $(SOLIDRT_HOME)/apps/player
PLAYER_SRC = $(PLAYER_DIR)/src/index.tsx
PLAYER_JS  = $(SOLIDRT_HOME)/packages/cli/player/index.sol.js

# The BSOD screen shown when an app fails to start.
BSOD_SRC = $(PLAYER_DIR)/src/bsod.tsx
BSOD_JS  = $(SOLIDRT_HOME)/packages/cli/player/bsod.sol.js

include $(SOLIDRT_HOME)/packages/cli/runtime/platform.mk

# The notices a distributed binary owes (the license texts of everything in
# it that is not ours), generated for the build they ship with and written
# next to the binaries of a platform package.
# $(1) = rust target triple, $(2) = the package folder
NOTICES = THIRD-PARTY-NOTICES.txt
define stage-notices
	bun $(SOLIDRT_HOME)/scripts/build-third-party-notices.ts --target $(1) --out $(2)/$(NOTICES)
endef

include Makefile.android
ifeq ($(findstring apple-darwin,$(HOST_TARGET)),apple-darwin)
include Makefile.darwin
else ifeq ($(findstring windows,$(HOST_TARGET)),windows)
include Makefile.windows
else
include Makefile.linux
endif

.DEFAULT_GOAL := client

.PHONY: lattice help player-bundle dist-clean clean

# Build both lattice host binaries (client + runtime). The root `make all`
# adds the flux binaries on top.
lattice: client runtime

# The monorepo packages both bundles bake in. Without these prerequisites a
# core/components surface change leaves a stale bundle importing a runtime
# export that no longer exists, failing at module link on engine start. The
# root bun.lock stands in for the inlined third-party deps (solid-js): a
# version bump rewrites it, so both bundles pick up the new runtime.
BUNDLE_DEPS = $(wildcard $(SOLIDRT_HOME)/packages/core/src/*.ts*) $(wildcard $(SOLIDRT_HOME)/packages/components/src/*.ts*) $(wildcard $(SOLIDRT_HOME)/bun.lock)

$(PLAYER_JS): $(wildcard $(PLAYER_DIR)/src/*.ts*) $(wildcard $(PLAYER_DIR)/src/parts/*.ts*) $(BUNDLE_DEPS)
	@echo "Bundling player (index.tsx -> packages/cli/player/)..."
	cd $(PLAYER_DIR) && bun run sol bundle $(PLAYER_SRC) --project -o $(dir $(PLAYER_JS))

$(BSOD_JS): $(BSOD_SRC) $(BUNDLE_DEPS)
	@echo "Bundling BSOD screen (bsod.tsx -> packages/cli/player/)..."
	cd $(PLAYER_DIR) && bun run sol bundle $(BSOD_SRC) --project -o $(dir $(BSOD_JS))

help:
	@echo "Host ($(HOST_TARGET)):"
	@echo "  client               Build the client (default)"
	@echo "  runtime              Build production runtime"
	@echo "  lattice              Build both client and runtime"
	@echo "  dist                 Build both plus the flux binaries into packages/$(HOST_PLATFORM)/"
	@echo ""
	@echo "Android dev (single ABI=$(ANDROID_ABI)):"
	@echo "  android-client       Build the client .so"
	@echo "  android-run          Build, install, launch, tail logcat"
	@echo "  android-run-armeabi-v7a  Same, for the 32-bit ABI"
	@echo "  android-runtime      Build the production runner APK (the sol pack --apk base)"
	@echo ""
	@echo "  The launched client auto-connects to DEV_SERVER=$(DEV_SERVER)."
	@echo "  Override it (10.0.2.2:$(DEV_PORT) for an emulator), or DEV_SERVER= for none."
	@echo ""
	@echo "Android dist (single ABI=$(ANDROID_ABI)):"
	@echo "  android-dist         Build the client and runner APKs into packages/android-$(ANDROID_ABI)/"
	@echo "  android-dist-armeabi-v7a  The same, for the 32-bit ABI"
	@echo ""
	@echo "  ANDROID_ABI=x86_64 builds for the emulator; it has no platform"
	@echo "  package, so the APKs stay in dist/android/ where sol android finds them."
	@echo ""
	@echo "The build flags (PROFILE=debug|release|release-opt, VIDEO=0, KTX2=0,"
	@echo "WASM_NATIVE=0, TEST=0, NDK_HOME) are the runtime's: make -C runtime help."
	@echo ""
	@echo "Player:"
	@echo "  player-bundle      Regenerate the player and BSOD bundles"
	@echo ""
	@echo "Clean:"
	@echo "  clean                Remove all build artifacts"
	@echo "  dist-clean           Remove dist/ only"

# -- Player -----------------------------------------
#
# Every runtime embeds the bundles at compile time via include_str!. The
# file targets rebuild them when any .tsx source in apps/player changes. Run
# `make player-bundle` to force a rebuild.

player-bundle: $(PLAYER_JS) $(BSOD_JS)

# -- Fonts --------------------------------------------

# The three Noto role defaults (sans/serif/mono). The go client embeds them
# (lattice lib.rs embedded_fonts); the production runtime ships no fonts, and
# `sol pack` appends these as trailer sections by default (see
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
	rm -rf $(SOLIDRT_HOME)/dist

# The runtime's clean covers its target/ (where the four builds go); the
# checkout's dist/ and the Gradle outputs are this file's.
clean:
	$(MAKE) -C $(SOLIDRT_HOME)/runtime clean
	rm -rf $(SOLIDRT_HOME)/dist
	rm -rf android/app/build android/build android/.gradle
