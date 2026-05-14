# lattice/Makefile

TIMESTAMP     := $(shell date '+%Y%m%d%H%M%S')
BUILD_VERSION  = BUILD_VERSION="$(TIMESTAMP)"

PROFILE            ?= release
CARGO_PROFILE_FLAG  = $(if $(filter release,$(PROFILE)),--release,)

DEFAULT_APP_SRC = $(CURDIR)/default-app/app.tsx
DEFAULT_APP_JS  = $(CURDIR)/default-app/app.srt.js

SRT_HOME ?= $(abspath $(CURDIR)/..)

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu = linux-x64-gnu
PLATFORM_aarch64-apple-darwin     = darwin-arm64
HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))
HOST_DIST      = $(SRT_HOME)/dist/$(HOST_PLATFORM)

include Makefile.x-android
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
	@echo "  solidrt-go           Build SolidRT Go (default)"
	@echo "  runtime              Build production runtime"
	@echo "  dist-linux           Build both into packages/linux-x64-gnu/"
	@echo ""
	@echo "Android (ABI=$(ANDROID_ABI)):"
	@echo "  x-solidrt-go-android   Build .so"
	@echo "  x-run-android          Build, install, launch, tail logcat"
	@echo ""
	@echo "Pass PROFILE=debug for a debug build (default: release)."
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
	rm -rf $(SRT_HOME)/dist

clean:
	rm -rf target $(SRT_HOME)/dist
	rm -rf android/app/build android/build android/.gradle
