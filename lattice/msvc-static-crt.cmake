# Toolchain file forcing the static CRT (/MT) on cmake-built dependencies
# (sdl3-sys, sdl3-mixer-sys). The cmake crate (0.1.58) injects -MT into the
# C flags when Rust builds with +crt-static but never sets
# CMAKE_MSVC_RUNTIME_LIBRARY; under policy CMP0091=NEW cmake then appends its
# /MD default after the user flags, and the last runtime flag wins - SDL
# objects silently come out /MD. A toolchain file is the one channel the
# cmake crate forwards from the environment (CMAKE_TOOLCHAIN_FILE), so this
# setting reaches every cmake configure. Wired up in Makefile.windows
# (WIN_CMAKE_ENV) and the winbox ~/winbuild cargo passthrough.
set(CMAKE_POLICY_DEFAULT_CMP0091 NEW)
set(CMAKE_MSVC_RUNTIME_LIBRARY MultiThreaded)
