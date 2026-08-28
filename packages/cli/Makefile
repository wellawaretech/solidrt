# The prebuilt dev server bundle a published CLI ships (dist/server.js,
# listed in package.json "files"; the release workflow builds it before
# publishing). A checkout does not need it: srt bundles the server per
# launch when the file is absent (src/main.ts), and a stale local copy
# would shadow every server edit, so never leave one around.
#
# The server (src/server/) is a flux script; the flux binary runs one plain
# JS file, so its TypeScript modules are bundled into one. The browser
# target keeps node builtins out; the flux: capability modules stay external
# (the runtime provides them).

dist/server.js: $(wildcard src/server/*.ts) $(wildcard src/types/*.d.ts)
	bun build src/server/main.ts --target=browser --format=esm --external 'flux:*' --outfile $@

# The dev console (apps/console) a published CLI ships pre-compiled: one
# .srtapp (manifest + bytecode + assets, no runner; `srt pack --app`) that
# `srt console` hands to the platform package's solidrt runner. Rebuilt when
# the console's own files change (not when @solidrt/core or components do);
# `srt console` never builds, so a checkout runs this after console edits.
CONSOLE := ../../apps/console

dist/console.srtapp: $(wildcard $(CONSOLE)/src/*) $(wildcard $(CONSOLE)/assets/*) $(CONSOLE)/package.json
	cd $(CONSOLE) && bun $(CURDIR)/bin/srt pack --app -o $(CURDIR)/$@

# The package demos (packages/*/demos/, one project per package) the CLI
# ships pre-bundled, so every install has every demo without installing the
# packages themselves. dist/demos/<pkg>/ is that project as the dev server
# serves it: its package.json (identity) and assets/, and per demo a
# <slug>/<slug>.srt.js bundle (`srt bundle`; a dir per demo because a
# bundle run owns its output's isolates/). `srt demo` starts the ordinary
# dev server on it with a client, so a demo shows up in the console like
# any app. Bundling needs only bun. The package.json is written last, so it
# stamps a complete build; the demoed package's src/ is a prerequisite
# because a demo of a changed package must not go stale.
DEMO_PACKAGES := $(patsubst ../%/demos/package.json,%,$(wildcard ../*/demos/package.json))

.PHONY: demos
demos: $(DEMO_PACKAGES:%=dist/demos/%/package.json)

.SECONDEXPANSION:
dist/demos/%/package.json: ../%/demos/package.json $$(wildcard ../$$*/demos/src/*.tsx) $$(wildcard ../$$*/demos/assets/*) $$(wildcard ../$$*/src/*)
	rm -rf $(@D)
	mkdir -p $(@D)
	cp -r ../$*/demos/assets $(@D)/
	for demo in ../$*/demos/src/*.tsx; do \
	  slug=$$(basename $$demo .tsx); \
	  (cd ../$*/demos && bun $(CURDIR)/bin/srt bundle src/$$slug.tsx --project -o $(CURDIR)/$(@D)/$$slug) || exit 1; \
	done
	cp ../$*/demos/package.json $@

.PHONY: clean
clean:
	rm -rf dist/server.js dist/console.srtapp dist/demos
