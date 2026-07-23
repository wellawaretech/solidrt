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
include("$ENV{ANDROID_NDK_ROOT}/build/cmake/android.toolchain.cmake")

# Force >=16 KB ELF LOAD-segment alignment on every shared lib built through the
# NDK here (libSDL3.so above all). Android 15+ flags libs whose LOAD segments are
# not 16 KB aligned as incompatible. The NDK linker default only became 16 KB in
# r27, and a stale cached .so built by an older NDK can otherwise get repackaged,
# so pin it explicitly instead of trusting the toolchain default.
set(CMAKE_SHARED_LINKER_FLAGS_INIT "${CMAKE_SHARED_LINKER_FLAGS_INIT} -Wl,-z,max-page-size=16384")