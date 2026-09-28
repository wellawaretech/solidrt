---
title: impellers links msvcrt on Windows, defeating crt-static
description: With static_link on Windows, impellers' build.rs emits cargo:rustc-link-lib=msvcrt, so our +crt-static solidrt-go.exe still resolves the C runtime from the DLLs and imports VCRUNTIME140.dll, VCRUNTIME140_1.dll and api-ms-win-crt-*, needing the Visual C++ redistributable to start.
project: impellers (github.com/coderedart/impellers)
versions: impellers 0.4.2 (latest release, 2026-05-28); line unchanged on master as of 2026-09-28
status: filed
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

## Our side until it lands

Nothing applied: our Windows builds keep importing the DLL CRT. Apply the
patch below only when we need a Windows exe that runs without the Visual C++
redistributable and upstream has not released the fix yet.

The patch, when needed:

1. In the fork, a branch `solidrt-patch`: `windows-static-crt` (the PR branch,
   master plus the fix) plus one commit that deletes the `flutter` submodule
   (`git rm flutter` and `.gitmodules`). The PR branch stays clean for
   upstream. Dropping the submodule matters: cargo checks out the submodules
   of a git dependency, so without it every machine that builds solidrt
   (laptop, builders, CI) would clone flutter/flutter. The crate does not
   need it: its bindings are pre-generated, `bindgen_live` (armv7 Android)
   uses the bundled `impeller.h`, and the Impeller binaries are downloaded.
2. In the root `Cargo.toml`, one line under the existing `[patch.crates-io]`,
   next to livekit-wakeword:
   `impellers = { git = "https://github.com/antoinevanwel/impellers", rev = "<patch commit>" }`.
   Cargo moves impellers to the git source in `Cargo.lock`; nothing else in
   the repo changes.
3. Check with the win32-x64-msvc builder (`objdump -p solidrt-go.exe | grep
   "DLL Name"` shows no vcruntime or api-ms-win-crt) and one other builder,
   since the patch applies to every platform.

Which base the branch uses does not matter: master is one commit past the
0.4.2 release (`169c3a4`, bumps the flutter submodule and `ENGINE_SHA`, edits
CONTRIBUTING.md), the crate version stays 0.4.2, and `build.rs` downloads the
prebuilt Impeller by crate version (release tag `a_0.4.2`), never reading
`ENGINE_SHA`.

When upstream releases the fix: bump the impellers pin (or drop the patch
line if it was applied), move this note to `resolved`, and check the
static-CRT comments in `lattice/Makefile.windows`, which describe the /MT
intent.
