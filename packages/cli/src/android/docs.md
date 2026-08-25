# srt android

{{ usage android }}

Installs the SolidRT client on a connected Android device (or emulator) over
adb and launches it against the dev server: the one serving the project (or
file) in the current directory, or `--port`. The server must run with
`--lan` so the device can reach it; an emulator reaches a loopback server
through its host alias. `--device` picks one of several connected devices
by serial or unique prefix.

adb is a system tool (Android Platform Tools) and is never bundled; the
APK comes from the `@solidrt/android-<abi>` dev dependency matching the
device's ABI, and the command says which to add when it is missing.
