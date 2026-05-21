WORKSPACE_ROOT := ..

PROFILE            ?= release
CARGO_PROFILE_FLAG  = $(if $(filter release,$(PROFILE)),--release,)

SRT_HOME ?= $(abspath $(CURDIR)/..)

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu  = linux-x64-gnu
PLATFORM_aarch64-apple-darwin      = darwin-arm64
PLATFORM_x86_64-pc-windows-msvc    = win32-x64-msvc
HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))
HOST_DIST      = $(SRT_HOME)/dist/$(HOST_PLATFORM)

HOST_EXT := $(if $(findstring windows,$(HOST_TARGET)),.exe,)

# $(1) = binary name (no extension)
define stage-bin
	@mkdir -p $(HOST_DIST)
	@cp $(WORKSPACE_ROOT)/target/$(PROFILE)/$(1)$(HOST_EXT) $(HOST_DIST)/
	@echo "Staged $(1)$(HOST_EXT) into $(HOST_DIST)/"
endef

.PHONY: build flux fluxc test clean

build: flux fluxc

flux:
	cargo build $(CARGO_PROFILE_FLAG) --bin flux
	$(call stage-bin,flux)

fluxc:
	cargo build $(CARGO_PROFILE_FLAG) --bin fluxc
	$(call stage-bin,fluxc)

test:
	cargo test

clean:
	cargo clean