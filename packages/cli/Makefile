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

.PHONY: clean
clean:
	rm -f dist/server.js dist/console.srtapp
