help all client runtime launcher-bundle dist-linux dist-darwin dist-windows client-android run-android run-android-armeabi-v7a dist-android dist-android-armeabi-v7a clean dist-clean download-fonts:
	$(MAKE) -C lattice $@

test:
	cargo test --workspace

format:
	cargo fmt --all

.PHONY: help all client runtime launcher-bundle dist-linux dist-darwin dist-windows client-android run-android run-android-armeabi-v7a dist-android dist-android-armeabi-v7a clean dist-clean download-fonts test format
