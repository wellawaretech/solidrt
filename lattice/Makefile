# lattice/Makefile

TIMESTAMP     := $(shell date '+%Y%m%d%H%M%S')
BUILD_VERSION  = BUILD_VERSION="$(TIMESTAMP)"

DEFAULT_APP_SRC = $(CURDIR)/default-app/app.tsx
DEFAULT_APP_JS  = $(CURDIR)/default-app/app.srt.js

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu = linux-x64-gnu
PLATFORM_aarch64-apple-darwin     = darwin-arm64
HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))
HOST_DIST      = $(CURDIR)/dist/$(HOST_PLATFORM)

include Makefile.android
ifeq ($(findstring apple-darwin,$(HOST_TARGET)),apple-darwin)
include Makefile.darwin
else
include Makefile.linux
endif

.DEFAULT_GOAL := solidrt-go

.PHONY: help default-app-bundle dist-clean clean

$(DEFAULT_APP_JS): $(wildcard $(CURDIR)/default-app/*.tsx)
	@echo "Bundling default app (app.tsx -> app.srt.js)..."
	cd $(CURDIR)/.. && bunx srt build $(DEFAULT_APP_SRC)

help:
	@echo "Host ($(HOST_TARGET)):"
	@echo "  solidrt-go           Build SolidRT Go (release, default)"
	@echo "  solidrt-go-debug     Build SolidRT Go (debug)"
	@echo "  runtime              Build production runtime (release)"
	@echo "  dist-linux           Build both into packages/linux-x64-gnu/"
	@echo ""
	@echo "Android (ABI=$(ANDROID_ABI)):"
	@echo "  solidrt-go-android        Build .so (release)"
	@echo "  solidrt-go-android-debug  Build .so (debug)"
	@echo "  run-android               Build, install, launch, tail logcat"
	@echo "  run-android-debug         Same with debug .so"
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

# -- Clean --------------------------------------------

dist-clean:
	rm -rf dist

clean:
	rm -rf target dist
	rm -rf android/app/build android/build android/.gradle
