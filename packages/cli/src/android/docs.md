# srt android

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

`--install` installs (or updates) the client first, from the
`@solidrt/android-<abi>` dev dependency matching the device's ABI; the
command says which package to add when it is missing. Without `--install`
the installed client is left as it is, and a note says when its version is
not the one that package carries - a client you built and installed
yourself is never replaced unless you ask.

adb is a system tool (Android Platform Tools) and is never bundled.
