# platform.mk - shared host-platform mapping, included by runtime.mk beside
# it (a runtime's build, in the checkout and in an app) and by the flux,
# lattice and website Makefiles ($(SOLIDRT_HOME)/packages/cli/runtime/platform.mk,
# after SOLIDRT_HOME is set). The PLATFORM_* map is the single source of
# truth for which host triples have a native platform package under
# packages/<platform>/.

HOST_TARGET := $(shell rustc -vV | sed -n 's/host: //p')

PLATFORM_x86_64-unknown-linux-gnu  = linux-x64-gnu
PLATFORM_aarch64-unknown-linux-gnu = linux-arm64-gnu
PLATFORM_aarch64-apple-darwin      = darwin-arm64
PLATFORM_x86_64-pc-windows-msvc    = win32-x64-msvc

HOST_PLATFORM := $(PLATFORM_$(HOST_TARGET))

# Where the host binaries are staged: <DIST_DIR>/<platform>/. DIST_DIR is the
# checkout's dist/ for its Makefiles; runtime.mk sets its own before
# including this file.
DIST_DIR ?= $(SOLIDRT_HOME)/dist

# Deferred (=, not :=) so an unmapped host aborts at the point of use with a
# real message instead of silently staging into "dist//". Targets that never
# expand HOST_DIST (e.g. Android cross builds) still work on unmapped hosts.
HOST_DIST = $(if $(HOST_PLATFORM),$(DIST_DIR)/$(HOST_PLATFORM),$(error Unsupported host target $(HOST_TARGET): add it to platform.mk))

HOST_EXT := $(if $(findstring windows,$(HOST_TARGET)),.exe,)

# Replace the staged binary $(2) with $(1) without writing into the existing
# file. A running executable holds its inode: Linux refuses in-place writes
# (ETXTBSY) and macOS may kill the process when its signature changes under
# it, so the new binary lands under a temp name and is renamed over the old
# path; the running client keeps the old inode until it exits, and the kernel
# frees it then. Windows cannot overwrite or delete a running .exe, but it can
# rename it aside: the previous binary becomes .old and is removed on the
# next build. Verified on the Windows box under MSYS2 (2026-08-25): all three
# cases pass (staged exe running; .old running; both running). Note Cygwin's
# rm/mv quietly move a locked file aside themselves, so under Git bash / MSYS2
# even a plain mv -f would work; the .old step keeps that explicit and avoids
# leaving the previous binary in the recycle bin on every build.
ifeq ($(HOST_EXT),.exe)
define replace-bin
	@cp $(1) $(2).tmp
	@rm -f $(2).old 2>/dev/null || true
	@if [ -e $(2) ]; then mv -f $(2) $(2).old 2>/dev/null || rm -f $(2); fi
	@mv $(2).tmp $(2)
endef
else
define replace-bin
	@cp $(1) $(2).tmp
	@mv -f $(2).tmp $(2)
endef
endif

# -- Android ---------------------------------------------------------------
#
# Every Android build is single-ABI, selected by ANDROID_ABI (arm64-v8a for
# devices, armeabi-v7a for the 32-bit ones, x86_64 for the emulator). The
# rust target and the clang triple (the NDK sysroot's lib dir name; armv7
# and x86 differ from rust's) of each.
ANDROID_ABI ?= arm64-v8a

RUST_TARGET_arm64-v8a   = aarch64-linux-android
RUST_TARGET_armeabi-v7a = armv7-linux-androideabi
RUST_TARGET_x86_64      = x86_64-linux-android
RUST_TARGET_x86         = i686-linux-android
ANDROID_TARGET          = $(RUST_TARGET_$(ANDROID_ABI))

CLANG_TRIPLE_arm64-v8a   = aarch64-linux-android
CLANG_TRIPLE_armeabi-v7a = arm-linux-androideabi
CLANG_TRIPLE_x86_64      = x86_64-linux-android
CLANG_TRIPLE_x86         = i686-linux-android
ANDROID_CLANG_TRIPLE     = $(CLANG_TRIPLE_$(ANDROID_ABI))
