---
title: SDL v4l2 camera enumeration does not terminate on stepwise frame-size ranges
description: SDL's V4L2 camera backend expands a stepwise frame-size range one step at a time, so a device advertising 32x32-16384x16384 step 2 costs ~67 million probes per format; on a Raspberry Pi 4 SDL_InitSubSystem(SDL_INIT_CAMERA) never returns, and because SDL_UDEV_Scan shares its callback list the main thread's gamepad init is dragged into the same loop.
project: SDL (github.com/libsdl-org/SDL)
versions: SDL 3.4.10 (via sdl3-sys 0.6.6+SDL-3.4.10); loop unchanged in upstream main as of 2026-08-26
status: acknowledged
link: https://github.com/libsdl-org/SDL/issues/15085
created: 2026-08-26
---

# SDL v4l2 camera enumeration does not terminate on stepwise frame-size ranges

Found 2026-08-26 on a Raspberry Pi 4 (Raspberry Pi OS, Debian 13, kernel
6.18.39, no camera attached). Every solidrt client on the box came up as a
black 1280x720 window and stayed there: `[alloy] GPU ready` and
`[srt] flux engine start` logged, the app's JS ran to completion (the demo's
scene built and uploaded its buffers), the control API answered `/tree` and
`/stats`, and yet `frame: 0, fps: 0, gpuPasses: 0, idleTicks: 0,
nodesPainted: 0` and every node in the tree, window included, measured 0x0.
Two threads pegged at 99% CPU, 197% total, for as long as the process lived.

## The loop

`src/camera/v4l2/SDL_camera_v4l2.c`, in the device probe:

```c
} else if ((frmsizeenum.type == V4L2_FRMSIZE_TYPE_STEPWISE) || (frmsizeenum.type == V4L2_FRMSIZE_TYPE_CONTINUOUS)) {
    ...
    for (int w = minw; w <= maxw; w += stepw) {
        for (int h = minh; h <= maxh; h += steph) {
            if (!AddCameraFormat(fd, &add_data, sdlfmt, colorspace, fmtdesc.pixelformat, w, h)) {
                break;
            }
        }
    }
    break;
}
```

A stepwise range is expanded pointwise. The Pi has 14 `/dev/videoN` nodes
(bcm2835-codec / isp / rpivid) even with no camera attached, and they
advertise, per `v4l2-ctl --list-formats-ext /dev/video12`:

```
[0]: 'YUYV' (YUYV 4:2:2)
        Size: Stepwise 32x32 - 16384x16384 with step 2/2
[1]: 'YVYU' ... same
... about 20 formats, all the same
```

That is `((16384-32)/2+1)^2` = about 67 million iterations per format, times
~20 formats, times several devices. Measured with `strace -c` on the init
thread: **230,466 ioctls in 6 seconds, all failing** (~38k/s), every one a
`VIDIOC_ENUM_FRAMEINTERVALS` walking the range:

```
ioctl(24, VIDIOC_ENUM_FRAMEINTERVALS, {index=0, pixel_format=RGBX32,
      width=12887, height=12019}) = -1 ENOTTY
... height=12020, 12021, 12022 ...
```

fd 24 was `/dev/video14`. At that rate the first format alone takes ~14
hours. This is not slow, it is unbounded.

## Two symptoms, one bug

Upstream issue [#15085](https://github.com/libsdl-org/SDL/issues/15085) is
the same loop, reported from a Magewell Pro Capture HDMI card where
`stepw`/`steph` are 1. There it OOM-kills the process, because the interval
ioctl succeeds and every iteration allocates a spec.

On the Pi that ioctl returns `ENOTTY`, so `AddCameraFormat` adds nothing and
allocates nothing. No OOM, no crash, no log line: just an ioctl storm that
never ends. Arguably the worse of the two, because nothing reports it.

## How the wedge escapes the init worker

`alloy/src/camera.rs` deliberately runs `SDL_InitSubSystem(SDL_INIT_CAMERA)`
on a dedicated `srt-camera-init` thread, on the assumption that a wedged
backend then costs one parked thread and nothing else. On Linux that holds
only conditionally. `SDL_UDEV_Scan` (`src/core/linux/SDL_udev.c`) matches three
subsystems:

```c
_this->syms.udev_enumerate_add_match_subsystem(enumerate, "input");
_this->syms.udev_enumerate_add_match_subsystem(enumerate, "sound");
_this->syms.udev_enumerate_add_match_subsystem(enumerate, "video4linux");
```

and dispatches every device it finds to *every* registered callback. So once
the camera subsystem has registered its callback, any udev scan re-runs the
whole camera enumeration - including the scan the main thread performs during
gamepad init. gdb on a wedged client, both threads in the same code:

```
Thread 1 "solidrt-go":
  ioctl -> AddCameraFormat -> MaybeAddDevice -> SDL_UDEV_Scan
        -> LINUX_JoystickInit -> SDL_InitJoysticks -> SDL_InitSubSystem
        -> alloy::gamepad::Gamepads::new -> alloy::app::App::run

Thread 2 "srt-camera-init":
  ioctl -> AddCameraFormat -> MaybeAddDevice -> SDL_UDEV_Scan
        -> SDL_CameraInit -> SDL_InitSubSystem
```

`App::run` never reaches its event pump, so no `Tick` is ever emitted, the UI
thread never gets a frame signal, and nothing is drawn. The window exists and
is mapped only because `RasterState::prime_window()` clears FBO 0 and swaps
once at raster-thread startup - hence a black window rather than no window.

This half is not in #15085 and appears unreported. It is not a race: any
`SDL_UDEV_Scan` after camera registration walks the video4linux devices
through the camera callback, by design.

The precise condition matters, because it is what makes the fix below work.
The worker DOES contain the wedge as long as no further `SDL_UDEV_Scan`
runs. Gamepad init is the scan that bit us: it happens inside `App::run` at
startup, and on this box it is slow enough (14 video nodes) to still be
running when the launcher's first render called into the camera. Hotplug
does NOT do the same: `SDL_UDEV_Poll` dispatches the one device that
changed, and `CameraUdevCallback` only acts on `SDL_UDEV_DEVICE_VIDEO_CAPTURE`
class devices, so a gamepad plugged in later never re-walks the video nodes.
Only another full scan - a later subsystem init - would, and nothing in the
startup sequence does one after scripts run. Keep camera init out of startup
and the wedge stays on its own thread.

## Confirming it is the camera and nothing else

`SDL_CAMERA_DRIVER=dummy` on the same binary, same box, same app:

```
[alloy] frame size 0x0 -> 1280x720
[alloy] window backbuffer is single-sample
frame: 283, fps: 22, nodes: 10, nodesPainted: 7, gpuPasses: 284, idleTicks: 27
```

V3D, GLES 3.1, Impeller and the app were never implicated.

## What we do about it

The launcher no longer enumerates cameras. Its scan button used to be gated
on `cameraDevices().length > 0` in both `home-screen.tsx` and
`connect-panel.tsx`, which started the camera subsystem on every client
start - and, on the Pi, did so while gamepad init was still scanning, so both
threads entered the loop at once. The button now shows unconditionally and
the camera subsystem starts only when the scan screen actually opens a
camera; a machine whose backend cannot enumerate stays fully usable as long
as nothing asks it to. See the comment on
`sdl_utils::camera_subsystem_init`.

Forcing `SDL_CAMERA_DRIVER=dummy` on Linux was implemented and then rejected
as too broad: it would forfeit working v4l2 cameras on every Linux desktop to
protect one class of board. `SDL_CAMERA_DRIVER` in the environment remains the
escape hatch either way.

Consequence to accept: on this Pi, pressing the scan button starts an
enumeration that never finishes and one core burns until the client exits.
The rest of the client is unaffected - measured 2026-08-26 with a probe app
that renders first and calls `listCameras()` eight seconds later:

```
  PID USER   S  %CPU  TIME+   COMMAND
60444 awel   R  99.9  0:15.28 srt-camera-init
60397 awel   S   0.0  0:00.57 solidrt-go      <- main loop, idle
... every other thread S
frame: 17, idleTicks: 841 -> 1346 over 25s, timeMs advancing
app log: "probe: calling listCameras()" / "probe: returned 0 cameras"
```

`listCameras()` returned `[]` immediately (the `Starting` state), the main
loop kept ticking, and the frame counter stayed put only because nothing on
screen changed. So the failure is one pegged core, not a dead client -
which is what the async worker was supposed to buy, and does buy once
nothing enumerates during startup.

An open while the subsystem is starting no longer rejects with "camera
subsystem is starting": `open_camera` defers the open and the pump performs
it once init reports in, or fails the session at `INIT_DEADLINE` (10 s, every
platform - init never waits on the user). On this Pi a scan press therefore
shows the viewfinder for 10 s and then the notice "camera subsystem did not
start within 10s"; on a healthy machine the first press simply works.

## Upstream

#15085 is open, filed 2026-02-21, assigned to icculus, milestoned **3.6.0**,
last activity 2026-06-16. slouken's reply: "Feel free to submit a PR to pick a
fixed rate for these sorts of cameras and we'll address it in a better way for
3.6.0." No PR has appeared, and the loop is byte-identical in upstream main as
of 2026-08-26.

A fix has a precedent inside the same function. For continuous frame
*intervals* SDL already refuses to expand the range:

```c
// FIXME: The current API does not enable exposing continuous ranges, so for
// now let's expose some common values that are within the range
```

and emits 24/30/50/60 fps if they fall inside it. Frame *sizes* never got the
same treatment. Doing for sizes what SDL already does for intervals - a
handful of common resolutions inside the range, honouring the step - is small
and follows the maintainers' stated preference.

Worth knowing when reporting the second half: bcm2835's stepwise frame-size
reporting is a known cross-project sore spot (it has broken GStreamer's
`v4l2src` and Chromium's WebRTC capture the same way, and the driver carries a
module parameter to stop advertising `V4L2_FRMSIZE_TYPE_STEPWISE` because so
many consumers mishandle it). SDL is in bad company rather than uniquely
wrong - but it is the only one of the three that hangs silently.
