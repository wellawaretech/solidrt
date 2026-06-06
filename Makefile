all solidrt-go runtime default-app-bundle dist-linux dist-darwin dist-windows dist-android clean dist-clean download-fonts:
	$(MAKE) -C lattice $@

test:
	cargo test --workspace

.PHONY: all solidrt-go runtime default-app-bundle dist-linux dist-darwin dist-windows dist-android clean dist-clean download-fonts test
