WORKSPACE_ROOT := ..

PROFILE            ?= release
ifeq ($(PROFILE),release)
  CARGO_PROFILE_FLAG = --release
else ifeq ($(PROFILE),debug)
  CARGO_PROFILE_FLAG =
else
  CARGO_PROFILE_FLAG = --profile $(PROFILE)
endif

SOLIDRT_HOME ?= $(abspath $(CURDIR)/..)

include $(SOLIDRT_HOME)/packages/cli/runtime/platform.mk

# $(1) = binary name (no extension)
define stage-bin
	@mkdir -p $(HOST_DIST)
	$(call replace-bin,$(WORKSPACE_ROOT)/target/$(PROFILE)/$(1)$(HOST_EXT),$(HOST_DIST)/$(1)$(HOST_EXT))
	@echo "Staged $(1)$(HOST_EXT) into $(HOST_DIST)/"
endef

.PHONY: build build-opt flux fluxc fluxrt test clean

build: flux fluxc fluxrt

# Stripped + fat LTO + single codegen unit. Slower build, optimised binary.
build-opt:
	$(MAKE) build PROFILE=release-opt

# The flux binary hosts the dev server and the bake tools, so it carries
# the texture encoder (flux:image encodeTexture) and every module a shipped
# runtime can (the native wasm lane, like the stock runtime). It is also
# what `sol test` runs test files on, so it alone carries flux:test. Both
# halves of flux:p2p are in the stock runtime, so they are here too, and the
# dev server's tunnel accepts on the endpoint.
# The interpreted wasm lane on a desktop is a cargo line with `wasm` alone
# (okf/notes/wasm-vs-js-throughput.md).
FLUX_FEATURES = --features ktx2,wasm-native,http,p2p,p2p-relay,test

# fluxrt is the runner a packed flux program ships on: the networking a
# headless program had before the modules became features (HTTP with TLS,
# and the p2p endpoint), no more.
FLUXRT_FEATURES = --features http,p2p

flux:
	cargo build $(CARGO_PROFILE_FLAG) $(FLUX_FEATURES) --bin flux
	$(call stage-bin,flux)

fluxc:
	cargo build $(CARGO_PROFILE_FLAG) --bin fluxc
	$(call stage-bin,fluxc)

fluxrt:
	cargo build $(CARGO_PROFILE_FLAG) $(FLUXRT_FEATURES) --bin fluxrt
	$(call stage-bin,fluxrt)

test:
	cargo test

clean:
	cargo clean