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

# lattice's clean covers lattice/target and dist/; flux's covers the
# workspace-root target/ that alloy, forge and flux build into.
clean:
	$(MAKE) -C lattice clean
	$(MAKE) -C flux clean

help client runtime launcher-bundle dist android-client android-run android-run-armeabi-v7a android-runtime android-dist android-dist-armeabi-v7a dist-clean download-fonts:
	$(MAKE) -C lattice $@

test:
	cargo test --workspace

format:
	cargo fmt --all

# lattice, flux and dist are also directory names at the repo root.
.PHONY: all lattice flux clean help client runtime launcher-bundle dist android-client android-run android-run-armeabi-v7a android-runtime android-dist android-dist-armeabi-v7a dist-clean download-fonts test format
