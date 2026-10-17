# @solidrt/cli/runtime/runtime.mk - how a SolidRT runtime is built and staged.
#
# A runtime is a cargo project over lattice: the stock program (runtime/ in
# the checkout) and a custom runtime beside an app (docs/runtime, "A custom
# runtime") alike. Its Makefile is one line,
#
#   include ../node_modules/@solidrt/cli/runtime/runtime.mk
#
# so the build moves with the @solidrt/cli package instead of with files a
# scaffold once copied. The project holds its Cargo.toml and its sources;
# everything else reaches the build from this folder: the toolchain pin
# (rust-toolchain.toml, through RUSTUP_TOOLCHAIN), cargo's config
# (config.toml: the Android page-size link flag, the rpaths, the profiles,
# passed with --config) and the cmake toolchains for Android and MSVC.
#
# Goals (run with make -C runtime from the app, or make here):
#   all              the player and the runtime for this host (the default)
#   client           the player, solidrt-go: what sol run and sol test drive
#   runtime          the production runtime, solidrt: what sol pack wraps
#   android          the player and runtime .so for ANDROID_ABI
#   android-client, android-runtime   one of the two
#   clean            remove target/ and dist/ here
#
# Staged into DIST_DIR (default: dist/ beside the Makefile), the layout sol
# reads when package.json names it ("solidrt": { "runtime": "runtime/dist" }):
#   <platform>/solidrt-go, <platform>/solidrt     platform: linux-x64-gnu,
#                                                 linux-arm64-gnu, darwin-arm64,
#                                                 win32-x64-msvc (platform.mk)
#   android/<abi>/libmain.so                      the player .so and the libs
#   android-runtime/<abi>/libmain.so              beside it (sol derives the
#                                                 APKs from the stock ones)
#   fonts.txt                                     the aliases of the program's
#                                                 font set, one per line
#                                                 (`solidrt --fonts`), written
#                                                 by the host goals; sol checks
#                                                 an app's solidrt.fonts
#                                                 against it
#
# Variables, all overridable on the command line:
#   PROFILE      release (default: fast, keeps symbols), debug, or release-opt:
#                the publish build, fat LTO and stripped, much slower; what
#                you ship
#   ANDROID_ABI  arm64-v8a (default), armeabi-v7a, x86_64 (the emulator)
#   NDK_HOME     the Android NDK (default: /opt/android-ndk)
#   TEST=0       leave the test host out of the player (default: in)
#   VERSION      what the runtime reports (default: the lattice tag in
#                Cargo.toml, else git describe)
#   DIST_DIR     where to stage (default: dist/ here)

# This folder, before any include moves MAKEFILE_LIST on.
BASE    := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
PACKAGE := $(abspath $(BASE)/..)

DIST_DIR ?= $(CURDIR)/dist
include $(BASE)/platform.mk

# A path for a native Windows program (cargo, cmake) from the MSYS2 shell
# make runs in there.
native = $(if $(findstring windows,$(HOST_TARGET)),$(shell cygpath -m $(1)),$(1))

# -- The project ----------------------------------------------------------

# The crate: its cdylib is lib<name>.so with dashes as underscores.
CRATE := $(shell sed -n 's/^name = "\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
LIB   := $(subst -,_,$(CRATE))

# A runtime builds against the SolidRT release its app's @solidrt packages
# are at (the two screens it embeds come from this package, made for that
# release): a lattice tag in Cargo.toml must be this package's version. The
# checkout pins neither (a path dependency, version 0.0.0), so there is
# nothing to compare.
LATTICE_TAG := $(shell sed -n 's/.*tag = "v\([^"]*\)".*/\1/p' Cargo.toml)
CLI_VERSION := $(shell sed -n 's/^  "version": "\([^"]*\)".*/\1/p' $(PACKAGE)/package.json)
ifneq ($(LATTICE_TAG),)
ifneq ($(CLI_VERSION),0.0.0)
ifneq ($(LATTICE_TAG),$(CLI_VERSION))
$(error Cargo.toml pins lattice at v$(LATTICE_TAG) and @solidrt/cli is $(CLI_VERSION); a runtime is built at the release its app is at, so bring the tag and the packages to one version)
endif
endif
endif

# What lattice::VERSION and every log line report; a build without it says
# 0.0.0-dev. The checkout's Makefiles pass their git describe.
VERSION ?= $(or $(LATTICE_TAG),$(shell git describe --tags --always --dirty 2>/dev/null | sed 's/^v//'),0.0.0-dev)

# -- The toolchain and cargo's config --------------------------------------

# The pinned toolchain, for every cargo and rustc this file runs; rustup
# installs it when missing. The Android goals add their target to it.
TOOLCHAIN := $(shell sed -n 's/^channel = "\([^"]*\)".*/\1/p' $(BASE)/rust-toolchain.toml)
export RUSTUP_TOOLCHAIN := $(TOOLCHAIN)

CONFIG = --config $(call native,$(BASE)/config.toml)

# -- Profile and features -------------------------------------------------

# release is the fast build that keeps symbols; release-opt (config.toml:
# fat LTO, one codegen unit, stripped) is the much slower publish build.
PROFILE ?= release
ifeq ($(PROFILE),release)
  CARGO_PROFILE_FLAG = --release
else ifeq ($(PROFILE),debug)
  CARGO_PROFILE_FLAG =
else
  CARGO_PROFILE_FLAG = --profile $(PROFILE)
endif

# The capabilities are the runtime crate's Cargo.toml's: lattice's default
# set unless its lattice line trims it (lattice/Cargo.toml), so the build
# passes no capability feature. The crate's own `go` feature (the player) is
# the one the manifest has, for the solidrt-go binary to require.
# The test host (the --test runner, sol:test, flux:test) is in the player,
# which is what sol test runs app tests on, never in the runtime.
TEST ?= 1

GO_FEATURES      := --features "go$(if $(filter 0,$(TEST)),, lattice/test)"
RUNTIME_FEATURES :=

# One target dir per feature set, so the player and the runtime never
# rebuild each other's lattice; the Android player shares the desktop
# player's (cargo keeps targets apart below it).
GO_TARGET_DIR              ?= target/go
RUNTIME_TARGET_DIR         ?= target/runtime
RUNTIME_ANDROID_TARGET_DIR ?= target/runtime-android

# -- Host ------------------------------------------------------------------

ifeq ($(findstring apple-darwin,$(HOST_TARGET)),apple-darwin)
# Statically linked SDL contains @available checks that compile to
# ___isPlatformVersionAtLeast calls. That symbol lives in clang's compiler-rt
# builtins, which rustc's -nodefaultlibs link never pulls in; plain release
# builds get it from Rust's compiler-builtins instead, but fat LTO
# (release-opt) internalizes it away since no Rust code references it. Link
# the archive explicitly so both profiles resolve it. RUSTFLAGS replaces
# the target rustflags of config.toml entirely, so the @executable_path
# rpath from there is repeated here.
CLANG_RT_OSX := $(shell clang -print-resource-dir)/lib/darwin/libclang_rt.osx.a
HOST_ENV = RUSTFLAGS="-C link-arg=-Wl,-rpath,@executable_path -C link-arg=$(CLANG_RT_OSX)"
# Impeller is linked statically on macOS, so the only dylibs beside the
# binary are ANGLE's (GL_LIBS below). Ad-hoc signed, like every local build.
# $(1) = target dir, $(2) = binary name
define stage-host
	@mkdir -p $(HOST_DIST)
	@cp $(1)/$(PROFILE)/$(2) $(HOST_DIST)/$(2).tmp
	@codesign --force --sign - $(HOST_DIST)/$(2).tmp
	@mv -f $(HOST_DIST)/$(2).tmp $(HOST_DIST)/$(2)
	@echo "Staged $(2) into $(HOST_DIST)/"
endef
else ifeq ($(findstring windows,$(HOST_TARGET)),windows)
# The prebuilt impeller libs link the static CRT (/MT). Force Rust std +
# cc-built code (+crt-static) and every -sys crate that runs its own cmake
# project (cmake defaults to /MD; a runtime module's whisper-rs-sys, say)
# to /MT too, or link.exe rejects the RuntimeLibrary mismatch. Such crates
# forward CMAKE_* env vars as cmake -D defines; CMP0091=NEW is required so
# CMAKE_MSVC_RUNTIME_LIBRARY is honored on a project with an old
# cmake_minimum_required.
# Do NOT append /OPT flags here: rustc already passes /OPT:REF,ICF and a
# later /OPT:NOREF,NOICF would win, re-enabling incremental-style layout
# (~7 MB of int3 padding, no COMDAT folding: solidrt.exe 52.7 -> 38.5 MB
# when the override was removed, 2026-07-19).
# The two plain env vars only reach crates that run cmake themselves. Deps built through the
# cmake crate (sdl3-sys; cmake 0.1.58 passes no MSVC runtime setting) never
# see them, and under CMP0091=NEW cmake appends its /MD default after the
# injected -MT, so the last flag wins and SDL objects silently come out /MD.
# The big client graph still links (UCRT import libs satisfy the __imp_*
# refs) but it is a static/DLL CRT mix, and small graphs fail to link
# outright. The toolchain file is the one channel the cmake crate reads
# from the env, forcing /MT on every cmake dep.
HOST_ENV = RUSTFLAGS="-Ctarget-feature=+crt-static" \
  CMAKE_POLICY_DEFAULT_CMP0091=NEW CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded \
  CMAKE_TOOLCHAIN_FILE=$(call native,$(BASE)/msvc-static-crt.cmake)
define stage-host
	@mkdir -p $(HOST_DIST)
	$(call replace-bin,$(1)/$(PROFILE)/$(2).exe,$(HOST_DIST)/$(2).exe)
	@echo "Staged $(2).exe into $(HOST_DIST)/"
endef
else
HOST_ENV =
define stage-host
	@mkdir -p $(HOST_DIST)
	$(call replace-bin,$(1)/$(PROFILE)/$(2),$(HOST_DIST)/$(2))
	@echo "Staged $(2) into $(HOST_DIST)/"
endef
endif

# The GL libraries a binary needs beside it on Windows and macOS (ANGLE),
# taken from the platform package the app installed; none on Linux, where
# the system GL serves. The checkout stages its own and passes GL_SRC=.
GL_SRC  ?= $(PACKAGE)/../$(HOST_PLATFORM)
GL_LIBS  = $(wildcard $(GL_SRC)/libEGL.* $(GL_SRC)/libGLESv2.*)

# The font set is the program's, the same in both binaries and on every
# target, so a host build lists it from the binary it just staged (the
# Android goals cannot run theirs).
# $(1) = label, $(2) = binary name, $(3) = features, $(4) = target dir
define host-build
	@echo "Building $(1) ($(PROFILE)) $(VERSION)"
	$(HOST_ENV) SOLIDRT_VERSION="$(VERSION)" SOLIDRT_PROFILE="$(PROFILE)" cargo build $(CONFIG) $(CARGO_PROFILE_FLAG) $(3) --bin $(2) --target-dir $(4)
	$(call stage-host,$(4),$(2))
	$(if $(GL_LIBS),@cp $(GL_LIBS) $(HOST_DIST)/)
	@$(HOST_DIST)/$(2)$(HOST_EXT) --fonts > $(DIST_DIR)/fonts.txt
	@echo "Listed the program's fonts in $(DIST_DIR)/fonts.txt"
endef

# -- Android ---------------------------------------------------------------

# ANDROID_ABI and the rust target and clang triple of each ABI: platform.mk.
NDK_HOME ?= /opt/android-ndk

# armv7 needs impellers' bindgen_live (see alloy/Cargo.toml) to point at the
# real 32-bit target. Use the bindgen-recognized BINDGEN_EXTRA_CLANG_ARGS_<target>
# suffix (matching Cargo's TARGET env var, hyphens to underscores) rather than
# the plain var: bindgen also runs for host-targeted build units in the same
# build (e.g. rquickjs-macro, a proc-macro crate that always builds for the
# host), and an unscoped value corrupts those instead of just armv7.
# --sysroot is required too: without it, clang's implicit host glibc fallback
# search path still gets appended after the NDK paths, and bionic's headers
# pull in incompatible glibc macros (__GLIBC_USE) through it.
BINDGEN_VAR_armeabi-v7a   = BINDGEN_EXTRA_CLANG_ARGS_armv7_linux_androideabi
BINDGEN_ARGS_armeabi-v7a  = --target=armv7a-linux-androideabi26 --sysroot=$(NDK_HOME)/toolchains/llvm/prebuilt/linux-x86_64/sysroot

# The NDK's LLVM tools, for the ELF reads and the strip below.
NDK_BIN = $(firstword $(dir $(wildcard $(NDK_HOME)/toolchains/llvm/prebuilt/*/bin/llvm-readelf)))
READELF = $(NDK_BIN)llvm-readelf
STRIP   = $(NDK_BIN)llvm-strip --strip-unneeded

# What cmake needs to build the native pieces (SDL, the video codecs) for
# Android: the NDK under both names, the NDK toolchain wrapped to default the
# ABI from the environment (android-abi.toolchain.cmake, for a build that
# passes no ABI of its own) for the rust target, and the ABI.
ANDROID_ENV = ANDROID_NDK_HOME=$(NDK_HOME) ANDROID_NDK_ROOT=$(NDK_HOME) \
  CMAKE_TOOLCHAIN_FILE_$(subst -,_,$(ANDROID_TARGET))=$(BASE)/android-abi.toolchain.cmake \
  CMAKE_ANDROID_ARCH_ABI=$(ANDROID_ABI) \
  $(if $(BINDGEN_VAR_$(ANDROID_ABI)),$(BINDGEN_VAR_$(ANDROID_ABI))="$(BINDGEN_ARGS_$(ANDROID_ABI))")

# Fail the build when a staged .so is not 16 KB page aligned: Android 15+
# flags such libs as incompatible, and the linker flags that prevent it
# (config.toml, android-abi.toolchain.cmake) have regressed silently before.
# Only the 64-bit ABIs are held to it; the NDK's own 32-bit libc++_shared.so
# is 4 KB aligned. sol checks the same before it derives an APK.
# $(1) = directory holding the staged .so files
ELF_MIN_ALIGN = 16384
ALIGNED_ABIS  = arm64-v8a x86_64
define check-elf-align
	$(if $(filter $(ANDROID_ABI),$(ALIGNED_ABIS)),@for so in $(1)/*.so; do \
	  for align in $$($(READELF) -lW $$so | awk '$$1 == "LOAD" { print $$NF }'); do \
	    if [ $$(($$align)) -lt $(ELF_MIN_ALIGN) ]; then \
	      echo "$$so: LOAD segment aligned to $$align; need >= $(ELF_MIN_ALIGN)"; exit 1; \
	    fi; \
	  done; \
	done; echo "Checked 16 KB ELF alignment in $(1)/")
endef

# Cross-compile the cdylib with cargo-ndk, then stage it as libmain.so (the
# name the Android shell loads) with the shared libs beside it: SDL, impeller,
# and the NDK's C++ runtime when a staged lib links it (the stock build does
# through impeller, a C++ module does), so the APK carries what is needed and
# nothing else. Every lib is staged stripped: an APK is what this folder
# becomes (sol swaps the libs into the stock one as they are; the checkout's
# Gradle build would strip them, a derived APK does not), the prebuilt
# impeller carries 220 MB of symbols, and a release libmain.so 18 MB. The
# symbols stay in the target dir for ndk-stack.
# $(1) = label, $(2) = features, $(3) = target dir, $(4) = dist subdir
define ndk-build
	@echo "Building ANDROID $(1) ($(PROFILE), $(ANDROID_ABI)) $(VERSION)"
	@rustup target add $(ANDROID_TARGET) --toolchain $(TOOLCHAIN)
	$(ANDROID_ENV) SOLIDRT_VERSION="$(VERSION)" SOLIDRT_PROFILE="$(PROFILE)" cargo ndk -t $(ANDROID_ABI) build $(CONFIG) $(CARGO_PROFILE_FLAG) $(2) --lib --target-dir $(3)
	@mkdir -p $(DIST_DIR)/$(4)/$(ANDROID_ABI)
	$(STRIP) -o $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libmain.so $(3)/$(ANDROID_TARGET)/$(PROFILE)/lib$(LIB).so
	$(STRIP) -o $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libSDL3.so $(3)/$(ANDROID_TARGET)/$(PROFILE)/libSDL3.so
	$(STRIP) -o $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libimpeller.so "$$(ls -t $(3)/$(ANDROID_TARGET)/$(PROFILE)/build/impellers-*/out/libimpeller.so | head -n 1)"
	@if $(READELF) -d $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libmain.so $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libSDL3.so $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libimpeller.so | grep -q 'NEEDED.*libc++_shared'; then \
	  $(STRIP) -o $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libc++_shared.so $(NDK_HOME)/toolchains/llvm/prebuilt/*/sysroot/usr/lib/$(ANDROID_CLANG_TRIPLE)/libc++_shared.so; \
	  echo "Staged libc++_shared.so beside it (a staged lib links it)"; \
	else \
	  rm -f $(DIST_DIR)/$(4)/$(ANDROID_ABI)/libc++_shared.so; \
	fi
	$(call check-elf-align,$(DIST_DIR)/$(4)/$(ANDROID_ABI))
	@echo "Staged the $(ANDROID_ABI) libs into $(DIST_DIR)/$(4)/$(ANDROID_ABI)/"
endef

# -- Goals -----------------------------------------------------------------

.DEFAULT_GOAL := all
.PHONY: all client runtime android android-client android-runtime help clean

all: client runtime

client:
	$(call host-build,SOLIDRT-GO,solidrt-go,$(GO_FEATURES),$(GO_TARGET_DIR))

runtime:
	$(call host-build,RUNTIME,solidrt,$(RUNTIME_FEATURES),$(RUNTIME_TARGET_DIR))

android: android-client android-runtime

android-client:
	$(call ndk-build,SOLIDRT-GO,$(GO_FEATURES),$(GO_TARGET_DIR),android)

android-runtime:
	$(call ndk-build,RUNTIME,$(RUNTIME_FEATURES),$(RUNTIME_ANDROID_TARGET_DIR),android-runtime)

help:
	@echo "Host ($(HOST_TARGET), staged into $(DIST_DIR)/$(HOST_PLATFORM)/):"
	@echo "  all              The player and the runtime (default)"
	@echo "  client           The player, solidrt-go (sol run, sol test)"
	@echo "  runtime          The production runtime, solidrt (sol pack)"
	@echo ""
	@echo "Android (ANDROID_ABI=$(ANDROID_ABI), staged into $(DIST_DIR)/android*/$(ANDROID_ABI)/):"
	@echo "  android          The player and runtime .so"
	@echo "  android-client   The player .so (sol android)"
	@echo "  android-runtime  The runtime .so (sol pack --apk)"
	@echo ""
	@echo "Pass PROFILE=debug for a debug build (default: release)."
	@echo "Pass PROFILE=release-opt for the publish build: fat LTO and stripped, much slower; what you ship."
	@echo "Pass TEST=0 to build the player without the test host (default: on)."
	@echo "The capabilities (video, ktx2, wasm) are the runtime's Cargo.toml's: lattice's default set, or what its lattice line lists."
	@echo "Pass VERSION=<v> for what the runtime reports (default: $(VERSION))."
	@echo ""
	@echo "  clean            Remove target/ and dist/ here"

clean:
	rm -rf target dist
