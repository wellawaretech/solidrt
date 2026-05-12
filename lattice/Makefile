# lattice/Makefile

TIMESTAMP     := $(shell date '+%Y%m%d%H%M%S')
BUILD_VERSION  = BUILD_VERSION="$(TIMESTAMP)"

DEFAULT_APP_SRC = $(CURDIR)/default-app/app.tsx
DEFAULT_APP_JS  = $(CURDIR)/default-app/app.srt.js
DEFAULT_APP_BIN = $(CURDIR)/dist/default-app.srt.bin
FLUX            = $(HOST_DIST)/flux
SRT_DEFAULT_BUNDLE ?= $(DEFAULT_APP_BIN)

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu = linux-x64-gnu
HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))
HOST_DIST      = $(CURDIR)/dist/$(HOST_PLATFORM)

include Makefile.linux
include Makefile.android

.DEFAULT_GOAL := solidrt-go

.PHONY: help flux default-app default-app-bundle dist-clean clean

help:
	@echo "Host ($(HOST_TARGET)):"
	@echo "  solidrt-go           Build Solidrt Go (release, default)"
	@echo "  solidrt-go-debug     Build Solidrt Go (debug)"
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

# -- Tooling ------------------------------------------

flux:
	cargo build -p flux
	@mkdir -p $(HOST_DIST)
	@cp target/debug/flux $(HOST_DIST)/

# -- Default app --------------------------------------
#
# The dev runner embeds default-app.srt.bin at compile time.
# Compiled from the committed app.srt.js — no bun tooling needed.
# Regenerate app.srt.js from app.tsx via `make default-app-bundle`.

default-app: flux
	@echo "Compiling default app..."
	@mkdir -p $(CURDIR)/dist
	$(FLUX) < $(DEFAULT_APP_JS) > $(DEFAULT_APP_BIN)

default-app-bundle:
	@echo "Bundling default app (app.tsx -> app.srt.js)..."
	cd $(CURDIR)/.. && bunx srt build $(DEFAULT_APP_SRC)

# -- Clean --------------------------------------------

dist-clean:
	rm -rf dist

clean:
	cargo clean
	cargo clean --target-dir target-dev
	rm -rf dist
	rm -rf android/app/build android/build android/.gradle
