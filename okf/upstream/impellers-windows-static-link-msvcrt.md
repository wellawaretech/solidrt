---
title: impellers links msvcrt on Windows, defeating crt-static
description: With static_link on Windows, impellers' build.rs emits cargo:rustc-link-lib=msvcrt, so our +crt-static solidrt-go.exe still resolves the C runtime from the DLLs and imports VCRUNTIME140.dll, VCRUNTIME140_1.dll and api-ms-win-crt-*, needing the Visual C++ redistributable to start.
project: impellers (github.com/coderedart/impellers)
versions: impellers 0.4.2; fixed in 0.4.3 (2026-09-29), our pin since 2026-09-29
status: resolved
link: https://github.com/coderedart/impellers/pull/7
created: 2026-09-28
---

# impellers links msvcrt on Windows, defeating crt-static

`lattice/Makefile.windows` builds the Windows client and runtime with
`-Ctarget-feature=+crt-static` (plus the cmake runtime settings), so that
everything matches the static CRT (/MT) of the prebuilt `impeller.lib` and
the exe runs without the Visual C++ redistributable. The exe does not: a
`solidrt-go.exe` built that way (0.0.64-1, and a build of 2026-09-27)
imports `VCRUNTIME140.dll`, `VCRUNTIME140_1.dll` and the UCRT
`api-ms-win-crt-*` set (`objdump -p solidrt-go.exe | grep "DLL Name"`). On a
Windows install without the redistributable (`VCRUNTIME140_1.dll` is not part
of a clean Windows) the client is expected not to start.

## Cause

impellers 0.4.2 `build.rs`, with the `static_link` feature on Windows:

```rust
// do we really need all of these?
for lib in [
    "advapi32", "Rpcrt4", "Shlwapi", "user32", "Gdi32", "Shell32", "Winmm", "msvcrt",
] {
    println!("cargo:rustc-link-lib={lib}");
}
```

`msvcrt.lib` (the DLL CRT's import library) ends up on the link command line.
Libraries named on the command line are searched before default libraries,
so every CRT symbol resolves against the DLL CRT, and the `/DEFAULTLIB:libcmt`
that `+crt-static` and every native object request goes unused.

Evidence, from the winbox builder (x86_64-pc-windows-msvc, MSVC 14.44):

- All 24 native `.lib` files in `target/go/release/build` request `libcmt`
  only (`dumpbin -directives`), `impeller.lib` included (`libcmt` and
  `libcpmt`). Nothing native asks for the DLL CRT.
- `cargo rustc ... -- --print link-args` for the final link shows exactly one
  `msvcrt.lib`, directly after impellers' system libraries.

## What does not work on our side

- `-C link-arg=/NODEFAULTLIB:msvcrt.lib`: it only removes default libraries
  (from object directives), not a library named on the command line. Verified,
  imports unchanged.
- A `[target.x86_64-pc-windows-msvc.impeller]` build-script override (the
  crate has `links = "impeller"`): it replaces the whole build script, which
  also downloads the prebuilt libraries and generates the bindings.

## Fix

Drop `msvcrt` from that list: `impeller.lib`'s own directives already pull the
static CRT, and a consumer on the default dynamic CRT gets `msvcrt` from
Rust's std anyway (not tested). Verified with a path-patched copy of
impellers 0.4.2 on the winbox builder: links cleanly, the exe imports no CRT
DLLs at all, and it starts. No rendering run yet.

Filed as https://github.com/coderedart/impellers/pull/7 from the fork
github.com/antoinevanwel/impellers (branch `windows-static-crt`, based on
master). The PR also mentions that the crate's `repository` field still points
to github.com/coderedart/flutter, which returns 404.

## Outcome

Merged 2026-09-28 (`f885a2d`, no review comments) and released as impellers
0.4.3 on 2026-09-29. Our pin moved `=0.4.2` -> `=0.4.3` the same day. No
local patch had been applied, so no workaround comes out, and the static-CRT
comments in `lattice/Makefile.windows` still describe the /MT intent
correctly.

0.4.3 is more than the fix: `build.rs` downloads the prebuilt Impeller by its
own `STATIC_MAJOR/MINOR/PATCH` constants (not the crate version, and never
`ENGINE_SHA`), and the release moves them from `a_0.5.14` to `a_0.5.15`,
engine `c177d531` -> `5d33e22e`. So the bump is also an Impeller engine
update on every platform. The Rust bindings in `src/` are unchanged, and
`a_0.5.15` carries the same ten platform zips.

Confirmed 2026-09-29 on the win32-x64-msvc builder (0.0.64-9-gf7292f62):
`objdump -p solidrt-go.exe | grep "DLL Name"` lists no vcruntime and no
api-ms-win-crt. The one API-set import left, `api-ms-win-core-synch-l1-2-0`,
is a Windows core API set (Rust std's WaitOnAddress), not the CRT.
