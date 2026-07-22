all client runtime launcher-bundle dist-linux dist-darwin dist-windows client-android run-android dist-android dist-android-armeabi-v7a clean dist-clean download-fonts:
	$(MAKE) -C lattice $@

test:
	cargo test --workspace

format:
	cargo fmt --all

.PHONY: all client runtime launcher-bundle dist-linux dist-darwin dist-windows client-android run-android dist-android dist-android-armeabi-v7a clean dist-clean download-fonts test format
