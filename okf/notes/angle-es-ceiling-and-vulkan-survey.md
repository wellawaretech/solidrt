---
title: ANGLE's ES 3.0 ceiling and per-platform Vulkan support
description: ANGLE caps both the Metal and D3D11 backends at ES 3.0 in its own source, so no compute shaders on macOS or Windows and rebuilding ANGLE would not change it; every native-driver target is already ES 3.1+, and every target measured has working Vulkan with a compute queue, including the 2017 Mali-T860 TV at Vulkan 1.0.52.
created: 2026-09-07
---

# ANGLE's ES 3.0 ceiling and per-platform Vulkan support

ARCHITECTURE.md states the rule ("GLES 3.0 is the feature ceiling, and ANGLE
is why"). This is the evidence underneath it, plus the survey that answers
the obvious follow-up: if ANGLE is the ceiling, what would replacing it buy.

Measured 2026-09-07. Companion to [graphics-backend-strategy](graphics-backend-strategy.md),
which decided the one-GLES-contract direction; this note is what the
measurements say about that decision's ceiling.

## The 3.0 request was never a decision

`alloy/src/gl/context.rs` asks for `set_context_version(3, 0)`. That line
arrived in `f151cf03 "sdl setup"` (2026-05-02), the initial SDL bring-up,
and has never been revisited. No note argues 3.0 over 3.1.

It is also not a cap on native drivers. The same request returns a **3.2**
context on Mesa: `partial_repaint_probe`, which uses byte-identical context
setup to the runtime, prints `OpenGL ES 3.2 Mesa 26.1.5-arch1.1` on the
Linux box. Every target where we talk to a real driver is already past 3.0:

| Target | GLES |
| --- | --- |
| Linux dev box, Mesa Intel RPL-P | 3.2 |
| Samsung SM-T500, Adreno 610 | 3.2 |
| Philips TPM171E, Mali-T860 | 3.2 |
| Raspberry Pi, V3D | 3.1 |

## ANGLE is the ceiling, by its own design

Both backends we use are capped at ES 3.0 in ANGLE's source, not by the
hardware and not by our build:

- Metal, `src/libANGLE/renderer/metal/mtl_common.h`:
  `constexpr gl::Version kMaxSupportedGLVersion = gl::Version(3, 0);`
  with `DisplayMtl::getMaxConformantESVersion()` returning
  `min(supported, 3.0)`.
- D3D11, `src/libANGLE/renderer/d3d/d3d11/renderer11_utils.cpp`,
  `GetMaximumClientVersion` returns `gl::Version(3, 0)` for every feature
  level including 11_0/11_1/12_x, commented "3.1 could theoretically be
  supported on FL 11.0+, but is not supported in ANGLE".

Upstream's position on Metal is that there are no plans to raise it: that
backend exists to serve WebGL in Chrome and Safari, and WebGL2 is exactly
ES 3.0. Their suggested route to compute is Dawn/WebGPU.

Probed on both boxes with a dlopen/LoadLibrary EGL probe that requests ES
3.0/3.1/3.2 in turn and then compiles and dispatches a real compute shader:

| Box | Backend | ES 3.1 |
| --- | --- | --- |
| M1 mac mini | default | no, EGL_BAD_MATCH |
| M1 mac mini | forced Metal | no, EGL_BAD_MATCH |
| M1 mac mini | forced OpenGL | no, EGL_BAD_MATCH |
| winbox RTX 3070 | default (D3D11) | no, EGL_BAD_MATCH |
| winbox RTX 3070 | forced D3D11 | no, EGL_BAD_MATCH |
| winbox RTX 3070 | forced OpenGL (NVIDIA GL 4.5) | **yes**, compute compiled, linked, dispatched, correct result |

ES 3.1 appears only where ANGLE passes through to a desktop GL driver that
already has compute (GL 4.3+). That is why Windows has it via the GL backend
and macOS does not: Apple's GL is frozen at 4.1, and desktop GL did not get
compute until 4.3.

Consequence worth stating plainly: **rebuilding ANGLE changes nothing.** Our
binaries are unzipped from the Electron distribution
(`lattice/Makefile.windows`, `lattice/Makefile.darwin`,
`ELECTRON_VERSION = v40.9.2`), which is worth knowing, but they are faithful
to upstream on this point rather than a degraded build.

### Side finding: macOS runs the GL backend, not Metal

The default display on macOS reports
`ANGLE (Apple, Apple M1, OpenGL 4.1 Metal - 90.5)`, i.e. ANGLE's OpenGL
backend over Apple's deprecated GL 4.1. Forcing Metal gives
`ANGLE Metal Renderer: Apple M1`.

This is our client's real path: `configure_opengl` never sets
`SDL_GL_EGL_PLATFORM`, SDL defaults `gl_config.egl_platform = 0`
(`SDL_video.c`), `SDL_cocoaopengles.m` passes it through, and `SDL_egl.c`
guards the whole `eglGetPlatformDisplay` block with `if (platform)`, falling
through to `eglGetDisplay(EGL_DEFAULT_DISPLAY)`.

So on macOS we currently run GLES through ANGLE through Apple's deprecated
GL driver, one hop more than intended, on a driver Apple has frozen.
`backlog/angle-present-fence-pacing.md` assumes the Metal backend and is
wrong on that point. This does not affect the ES ceiling (both backends cap
at 3.0), but it is a live correctness and deprecation concern of its own.

## Vulkan is available everywhere, including the oldest device

The follow-up question is whether a single Vulkan backend could replace
ANGLE outright. On hardware grounds, yes. Probed with a small
`vkCreateInstance` plus queue-family probe:

| Box | Vulkan | Compute queue |
| --- | --- | --- |
| Linux dev box, Intel RPL-P, Mesa | 1.4.354 | yes |
| winbox, RTX 3070 | loader 1.4.321, device 1.4.325 | yes |
| Philips TPM171E TV, Mali-T860, Android 8, armv7 | **1.0.52** | **yes**: family 0, flags 0x7 GRAPHICS COMPUTE TRANSFER, count 2 |

The TV was the expected blocker and is not one. It reports
`android.hardware.vulkan.level=1`, `version=4194307` (1.0.3), ships a vendor
driver at `/vendor/lib/hw/vulkan.mt5891.so` (71 MB), and a real
`vkCreateInstance` succeeds. Compute is core in Vulkan 1.0, so a conformant
driver must expose it.

Caveats that would shape a Vulkan floor:

- The TV is Vulkan **1.0 only** (no `vkEnumerateInstanceVersion`). A 1.0
  floor means no core subgroup operations and no core `VK_KHR_maintenance1`,
  whose negative viewport height is the usual Y-flip; on 1.0 that is an
  extension needing a fallback.
- The TV reports **0 instance layers**. No validation layers on device, so
  on-device Vulkan debugging needs them bundled.
- Advertised is not exercised. A compute-capable queue is confirmed; no
  compute shader has been dispatched on the Mali. Its driver dates to 2021
  (Midgard r20p0 era) and quality is unknown.
- Untested: Adreno 610 tablet, Pi V3D, macOS, iOS.

On Apple, Vulkan means a translation layer either way: MoltenVK (Brenwill
Workshop, Apache 2.0, layered Vulkan 1.4, explicitly not fully conformant,
portability subset) or KosmicKrisp (LunarG, in Mesa, Khronos-conformant
Vulkan 1.3 on Apple Silicon, macOS 15+, iOS in progress). The gain over
ANGLE is not that translation disappears, it is that the translator does not
cap the feature set.

## Reproducing

The probes were scratch C files, not committed. Both are ~250 lines and take
the same shape: dlopen (`libEGL`/`libGLESv2`, or `libvulkan.so`) with the
constants spelled out inline so no headers or import libraries are needed,
then request each version in turn and report what comes back.

- EGL probe: request `EGL_CONTEXT_MAJOR_VERSION` 3 with
  `EGL_CONTEXT_MINOR_VERSION` 0, 1 and 2 against the default display and
  against each `EGL_PLATFORM_ANGLE_TYPE_*` display; on a 3.1 context,
  compile a `#version 310 es` compute shader, dispatch it and read the SSBO
  back to prove it actually runs.
- Vulkan probe: `vkCreateInstance`, `vkEnumeratePhysicalDevices`,
  `vkGetPhysicalDeviceQueueFamilyProperties`, checking for
  `VK_QUEUE_COMPUTE_BIT`. Cross-compiled for armeabi-v7a with the NDK for
  the TV and pushed to `/data/local/tmp`.

Getting the answer required forcing backends explicitly. A default-display
probe alone would have reported "ES 3.0" on both boxes and hidden both the
GL-backend passthrough on Windows and the wrong-backend finding on macOS.
