---
type: backlog-item
title: Android client forgets its dev-server address
description: The dev-server address only reaches the client as a launch-intent extra, so any relaunch that does not come from the CLI (the device's own launcher, a crash, a reboot) starts into apps/default and never reconnects; recovery needs adb.
status: deferred
timestamp: 2026-07-27T00:00:00Z
---

# Android client forgets its dev-server address

Split out of idle-tick-gpu-backlog-runaway.md.

`spawnAndroidClient` (packages/cli/src/dev-android.ts) hands the client the
dev-server address as the `srt_dev_server` launch-intent extra, which
MainActivity forwards to native argv as `--dev-server`. Nothing persists it. So
the address survives exactly as long as that process does.

Hit twice during the 2026-07-27 session. Restarting the client to get a clean
measurement baseline - `am force-stop` then launching from the TV's own
launcher - brought the app up against `apps/default` with no dev-server
connection and no path back from the couch. Recovery was:

```
adb shell am start -n com.solidrt.go/com.solidrt.app.MainActivity \
  --es srt_dev_server 192.168.2.69:34884
```

which requires knowing the host's LAN IP, that `DEV_PORT` is 0x8844, and that
the extra exists at all. Note also that `am start` on an already-running task
silently does not deliver the extra ("Activity not started, its current task has
been brought to the front"), so a force-stop first is mandatory - another thing
you have to know.

Every non-CLI relaunch has this problem, not just deliberate restarts: the
device's launcher, a crash, an OS-initiated kill, a reboot. On a TV, where the
launcher is the natural way to start anything, that is the common case rather
than the edge case.

## Shape of the fix

Persist the last-used dev-server address client-side and dial it on start when
no extra is supplied, with the extra continuing to win when present so the CLI
can always retarget. The client already has per-client storage under
`files/client0/` for exactly this kind of state.

Worth pairing with a visible fallback: if the stored address fails to connect,
say so on screen rather than silently running the default app, since "it
launched but is not my app" is the confusing part.

Related: client-build-info.md, dev-state-across-reloads.md.
