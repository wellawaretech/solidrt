---
title: Bun reports a Windows child's exit status as its low byte
description: On Windows, Bun.spawn, spawnSync and node:child_process narrow a child's 32-bit exit status to a byte, so a crash status like 0xC0000005 reads as a plain exit 5; the CLI cannot tell a crashed client from one that exited. Fix PR open upstream since 2026-07.
project: bun (github.com/oven-sh/bun)
versions: bun 1.3.14 (verified on the winbox 2026-10-19); still narrowed in 1.4.3 and on main (src/spawn/process.rs, on_exit_uv)
status: filed
link: https://github.com/oven-sh/bun/pull/33720
created: 2026-10-19
---

# Bun reports a Windows child's exit status as its low byte

A Windows process that dies of an exception exits with the NTSTATUS as its
exit code: `0xC0000005` for an access violation, `0xC0000409` for a
fast-fail abort, `0xC00000FD` for a stack overflow. Bun stores a child's
exit code as a `u8`, so every one of these collapses to its low byte, and
the result is indistinguishable from a clean `process.exit(N)`:

| the child exited with | Bun reports | Node reports |
|---|---|---|
| `0xC0000005` (access violation) | 5 | 3221225477 |
| `0xC0000409` (fast-fail abort) | 9 | 3221226505 |
| `0xC00000FD` (stack overflow) | 253 | 3221225725 |

Verified with `cmd.exe /c exit -1073741819`: `Bun.spawn` (`exited`,
`exitCode`), `Bun.spawnSync` and `node:child_process` (`spawn`,
`spawnSync`) all give 5; `signalCode` is null. Bun also keeps Windows
Error Reporting from recording a crash of its child (no Application-log
event, no dump), so the crash leaves no trace at all.

## What it costs us

`sol render`, `sol client`, `sol console` and `sol test` run the client
under bun, so a client crash on Windows was a silent exit 5 - a long
bisection for a reporter whose morph-target draw crashed in ANGLE. The
dev server runs on flux, whose `flux:subprocess` keeps the full status
(`-1073741819`), so only `sol run` could name it.

## Our workaround

The runtime names its own crash (`forge::process::report_crashes`): a
last-chance exception filter writes `[sol] Crashed: access violation
(0xC0000005) reading 0x20 in libGLESv2.dll+0x2E2F16` to stderr before the
process dies, which every spawner's log shows. The CLI's `crashReason`
(`packages/cli/src/server/crash.ts`) names the status where the spawner
reports it: signals everywhere, the hex status under flux.

## Upstream

The fix is open as oven-sh/bun#33720 ("spawn: preserve full 32-bit exit
codes on Windows", 2026-07-08): widens the exit code to `u32`, CI green,
unreviewed as of 2026-10-19. Bun's `Exited` already carries the untruncated
value in an internal `raw: u32` field; it is not exposed. Once it merges
and we are on that bun, `crashReason` names Windows crashes under the bun
commands too; the runtime line stays, it is what names the module.
