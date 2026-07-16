# Static docs website: content/ -> dist/, built by a flux script
# (see okf/plans/website.md). Needs the host flux binary in $(SRT_HOME)/dist
# (make runtime from the repo root).

SRT_HOME ?= $(abspath $(CURDIR)/..)

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu  = linux-x64-gnu
PLATFORM_aarch64-apple-darwin      = darwin-arm64
PLATFORM_x86_64-pc-windows-msvc    = win32-x64-msvc
HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))
HOST_DIST      = $(SRT_HOME)/dist/$(HOST_PLATFORM)

HOST_EXT := $(if $(findstring windows,$(HOST_TARGET)),.exe,)

build:
	bun install
	bun $(SRT_HOME)/packages/cli/bin/srt bundle --flux src/build.ts
	$(HOST_DIST)/flux$(HOST_EXT) src/build.flux.js

clean:
	rm -rf dist src/build.flux.js

.PHONY: build clean
