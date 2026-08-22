# Static docs website: content/ -> dist/, built by a flux script
# (see okf/plans/website.md). Needs the host flux binary in $(SRT_HOME)/dist
# (make runtime from the repo root).

SRT_HOME ?= $(abspath $(CURDIR)/..)

include $(SRT_HOME)/platform.mk

build:
	bun install
	bun $(SRT_HOME)/packages/cli/bin/srt bundle --flux src/build.ts
	$(HOST_DIST)/flux$(HOST_EXT) dist/bundle/build.flux.js

clean:
	rm -rf dist

.PHONY: build clean
