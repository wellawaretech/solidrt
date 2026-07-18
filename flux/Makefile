WORKSPACE_ROOT := ..

PROFILE            ?= release
ifeq ($(PROFILE),release)
  CARGO_PROFILE_FLAG = --release
else ifeq ($(PROFILE),debug)
  CARGO_PROFILE_FLAG =
else
  CARGO_PROFILE_FLAG = --profile $(PROFILE)
endif

SRT_HOME ?= $(abspath $(CURDIR)/..)

include $(SRT_HOME)/platform.mk

# $(1) = binary name (no extension)
define stage-bin
	@mkdir -p $(HOST_DIST)
	@cp $(WORKSPACE_ROOT)/target/$(PROFILE)/$(1)$(HOST_EXT) $(HOST_DIST)/
	@echo "Staged $(1)$(HOST_EXT) into $(HOST_DIST)/"
endef

.PHONY: build build-opt flux fluxc fluxrt test clean

build: flux fluxc fluxrt

# Stripped + fat LTO + single codegen unit. Slower build, optimised binary.
build-opt:
	$(MAKE) build PROFILE=release-opt

flux:
	cargo build $(CARGO_PROFILE_FLAG) --bin flux
	$(call stage-bin,flux)

fluxc:
	cargo build $(CARGO_PROFILE_FLAG) --bin fluxc
	$(call stage-bin,fluxc)

fluxrt:
	cargo build $(CARGO_PROFILE_FLAG) --bin fluxrt
	$(call stage-bin,fluxrt)

test:
	cargo test

clean:
	cargo clean