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

.PHONY: clean
clean:
	rm -f dist/server.js
