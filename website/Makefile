# Static docs website: ../docs and the package docs -> dist/site/, built by a
# flux script (see okf/plans/website.md). Needs the host flux binary in
# $(SOLIDRT_HOME)/dist (make runtime from the repo root).
#
# make build  renders the site into dist/site/
# make dev    serves it on PORT from memory, rebuilding it and reloading the
#             browser on every change to its sources

SOLIDRT_HOME ?= $(abspath $(CURDIR)/..)

include $(SOLIDRT_HOME)/packages/cli/runtime/platform.mk

# Port of the dev server.
PORT ?= 3000
# Exit status of the dev server asking to be re-bundled and started again,
# after a change to the generator itself (RESTART_EXIT in src/build.ts).
RESTART_EXIT = 75

BUNDLE = bun $(SOLIDRT_HOME)/packages/cli/bin/sol bundle --flux src/build.ts
GENERATOR = $(HOST_DIST)/flux$(HOST_EXT) dist/bundle/build.flux.js

build:
	bun install
	$(BUNDLE)
	rm -rf dist/site
	$(GENERATOR)

dev:
	bun install
	while $(BUNDLE) && { $(GENERATOR) --dev $(PORT); [ $$? -eq $(RESTART_EXIT) ]; }; do :; done

clean:
	rm -rf dist

.PHONY: build dev clean
