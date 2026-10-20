# Root Makefile. Host-native goals only: Android and packaging stay explicit.

# Everything buildable on the host: the two lattice binaries and the three
# flux binaries. Every platform package ships all five.
all: lattice flux

# solidrt-go (client) + solidrt (runtime).
lattice:
	$(MAKE) -C lattice lattice

# flux + fluxc + fluxrt. flux's collective goal is `build` because `flux` is
# taken by the single binary there.
flux:
	$(MAKE) -C flux build

# The apps in apps/, each built into whatever carries it: the console packed
# for the CLI (`sol console` hands the .solapp to the runner), the player and
# its BSOD screen bundled for the go client to embed. `make client` builds
# the player bundles because it compiles them in; nothing but this builds the
# console, so run it after a console, core or components change.
apps:
	$(MAKE) -C packages/cli console
	$(MAKE) -C lattice player-bundle

# lattice's clean covers runtime/target (the four runtime builds) and dist/;
# flux's covers the workspace-root target/ that alloy, forge and flux build
# into.
clean:
	$(MAKE) -C lattice clean
	$(MAKE) -C flux clean

help client runtime player-bundle dist android-client android-run android-run-armeabi-v7a android-runtime android-dist android-dist-armeabi-v7a dist-clean download-fonts:
	$(MAKE) -C lattice $@

test: manifests-check
	cargo test --workspace

# Every manifest in the repository, read as cargo reads them when a project
# depends on lattice by git: cargo parses each Cargo.toml in the checkout
# (the scaffold's `sol init --runtime` does exactly that), so one it rejects,
# a template with placeholders or a stray scratch crate, breaks every such
# build while the workspace here builds fine. --no-deps --offline reads the
# manifest alone, in milliseconds; tracked and untracked-unignored files, so
# a new one is caught before it is committed.
manifests-check:
	@for m in $$(git ls-files --cached --others --exclude-standard -- 'Cargo.toml' '*/Cargo.toml'); do \
	  [ -f "$$m" ] || continue; \
	  cargo metadata --no-deps --offline --format-version 1 --manifest-path "$$m" > /dev/null || exit 1; \
	done

# The renderer seam (okf/plans/renderer-seam.md): the `impellers` crate is
# named only by alloy's Impeller module, so a replacement renderer is a
# second module behind the same contract and nothing else in the workspace
# changes. Fails on any other Rust source that names it.
renderer-seam-check:
	@found=$$(grep -rlE "impellers::|use impellers\b" --include='*.rs' alloy/src flux/src lattice/src runtime/src | grep -v '^alloy/src/renderer/impeller.rs$$'); \
	if [ -n "$$found" ]; then echo "impellers is named outside alloy/src/renderer/impeller.rs:"; echo "$$found"; exit 1; fi

# The whole workspace, each crate at its own edition. The workspace is
# rustfmt-clean, so this touches only what you changed.
format:
	cargo fmt --all

# Lists the files `format` would change, and fails if there are any.
format-check:
	cargo fmt --all --check -- -l

# lattice, flux, apps and dist are also directory names at the repo root.
.PHONY: all lattice flux apps clean help client runtime player-bundle dist android-client android-run android-run-armeabi-v7a android-runtime android-dist android-dist-armeabi-v7a dist-clean download-fonts test format format-check manifests-check renderer-seam-check
