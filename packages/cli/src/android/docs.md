# sol android

{{ usage android }}

Launches the SolidRT client installed on a connected Android device (or
emulator) over adb against the dev server: the one serving the project (or
file) in the current directory, or `--port`. A running instance is restarted
so it picks the server up. The command then waits a few seconds for the
client to appear on the server and reports its client id. The server must
run with `--lan` so the device can reach it; an emulator reaches a loopback
server through its host alias. Without a running server the client starts
on its own, into the player (`--port` must name a live server). With several
devices connected a terminal asks which ones (all preselected, so enter
launches on every device); `--device` picks one by serial or unique prefix
(a script must).

A device without the client gets it installed first, from the project's
Android target for the device's ABI: the `@solidrt/android-<abi>` dev
dependency, which carries that ABI's client and its runner (the base of
`sol pack --apk`). A project with no target yet gets a picker the first
time (which devices it targets: arm64-v8a for current phones and tablets,
armeabi-v7a for older phones and many Android TVs, x86_64 for emulators and
Chromebooks); a device whose ABI is not among the targets is offered its
package. The targets are ordinary dev dependencies, so `bun add -d` and
`bun remove` change them later. An installed client whose version is not
the one the project's package carries is updated after asking - a client
you built and installed yourself stays unless you say so. `--install`
reinstalls regardless.

`--apk` works on the app instead of the client: it packs it (`sol pack
--apk`, one APK per target) and installs and launches on each device the
APK built for its ABI, nothing dev-flavored. `sol android <file.apk>`
installs and launches an APK you already have.

`--census` does not launch anything: it reads the compositor's own record
of the client's presents (`dumpsys SurfaceFlinger --latency`) and prints
the present intervals in refreshes (the cadence the panel showed), their
p50/p90, and the per-frame GPU span (frameReady minus queue time, the
frame's GPU work on a tiled GPU). These are the figures every Android
pacing and paint-cost verdict is measured with, because no runtime counter
can be wrong about them. `--clear` empties the history first and
`--seconds <N>` waits that long before reading, so `sol android --census
--clear --seconds 5` while you drive the app (by hand, or over the control
API) measures exactly that interaction; without them the reading is
whatever the compositor's short ring still holds. `--layer` picks a layer
by substring (a packed app, another surface); the default is the client's
SurfaceView layer, which exists only while the app is on screen.

adb is a system tool (Android Platform Tools) and is never bundled.
