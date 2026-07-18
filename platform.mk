# platform.mk - shared host-platform mapping, included by the flux, lattice,
# and website Makefiles (include $(SRT_HOME)/platform.mk, after SRT_HOME is
# set). The PLATFORM_* map is the single source of truth for which host
# triples have a native platform package under packages/<platform>/.

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu  = linux-x64-gnu
PLATFORM_aarch64-unknown-linux-gnu = linux-arm64-gnu
PLATFORM_aarch64-apple-darwin      = darwin-arm64
PLATFORM_x86_64-pc-windows-msvc    = win32-x64-msvc

HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))

# Deferred (=, not :=) so an unmapped host aborts at the point of use with a
# real message instead of silently staging into "dist//". Targets that never
# expand HOST_DIST (e.g. Android cross builds) still work on unmapped hosts.
HOST_DIST = $(if $(HOST_PLATFORM),$(SRT_HOME)/dist/$(HOST_PLATFORM),$(error Unsupported host target $(HOST_TARGET): add it to platform.mk))

HOST_EXT := $(if $(findstring windows,$(HOST_TARGET)),.exe,)
