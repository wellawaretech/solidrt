# Static docs website: content/ -> dist/, built by a flux script
# (see okf/plans/website.md). Needs the host flux binary in $(SOLIDRT_HOME)/dist
# (make runtime from the repo root).

SOLIDRT_HOME ?= $(abspath $(CURDIR)/..)

include $(SOLIDRT_HOME)/platform.mk

build:
	bun install
	bun $(SOLIDRT_HOME)/packages/cli/bin/sol bundle --flux src/build.ts
	$(HOST_DIST)/flux$(HOST_EXT) dist/bundle/build.flux.js

clean:
	rm -rf dist

.PHONY: build clean
