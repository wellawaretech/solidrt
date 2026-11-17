# Third-party licenses

License texts of software that ships in SolidRT binaries and carries no
license file anywhere the build can read it from: a prebuilt library, or a
library taken from another project's archive. Everything else (the Rust
crates, the libraries built from vendored sources) brings its own files.

`scripts/build-third-party-notices.ts` reads these, with the rest, into the
`THIRD-PARTY-NOTICES.txt` of each platform package; its `NATIVE` table says
what ships where. The dist goals run it (`lattice/Makefile`, `stage-notices`).

| Folder | What | Taken from | Refresh when |
|---|---|---|---|
| `angle/` | ANGLE's license; `libEGL` and `libGLESv2` ship on Windows and macOS | `LICENSE` of https://github.com/google/angle | the Electron version the libraries are taken from is bumped (`ELECTRON_VERSION`) |
| `libcxx/` | libc++ of the LLVM project; `libc++_shared.so` ships in the Android APK | `libcxx/LICENSE.TXT` of https://github.com/llvm/llvm-project | the NDK is bumped |

Adding a library that ships: add its row to `NATIVE` in the script, and its
license text here only if no file of its own is in reach of the build.
