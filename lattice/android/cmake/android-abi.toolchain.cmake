# Wraps the NDK toolchain to default ANDROID_ABI from the environment.
#
# Some -sys crates (whisper-rs-sys) run cmake without passing -DANDROID_ABI;
# the NDK toolchain then defaults to armeabi-v7a, which clashes with the
# target flags the cmake crate injects. Defining CMAKE_ANDROID_ARCH_ABI alone
# does not fix it: try_compile sub-projects only re-receive ANDROID_ABI (see
# CMAKE_TRY_COMPILE_PLATFORM_VARIABLES in the NDK toolchain), so the compiler
# checks still configure for armeabi-v7a and fail.
if(NOT ANDROID_ABI AND DEFINED ENV{CMAKE_ANDROID_ARCH_ABI})
  set(ANDROID_ABI "$ENV{CMAKE_ANDROID_ARCH_ABI}")
endif()
# The legacy NDK toolchain (the NDK's default, pinned here) is what the linker
# flag append below relies on: it defines CMAKE_SHARED_LINKER_FLAGS as a plain
# variable, and it creates the cache entry empty, so a CMAKE_SHARED_LINKER_FLAGS_INIT
# set here would be silently ignored.
set(ANDROID_USE_LEGACY_TOOLCHAIN_FILE ON)
include("$ENV{ANDROID_NDK_ROOT}/build/cmake/android.toolchain.cmake")

# Force >=16 KB ELF LOAD-segment alignment on every shared lib built through the
# NDK here (libSDL3.so above all). Android 15+ flags libs whose LOAD segments are
# not 16 KB aligned as incompatible. The NDK linker default only became 16 KB in
# r28, so pin it explicitly instead of trusting the toolchain default. The
# Makefile.android staging steps verify the result (check-elf-align).
string(APPEND CMAKE_SHARED_LINKER_FLAGS " -Wl,-z,max-page-size=16384")
