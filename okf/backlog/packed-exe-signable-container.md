---
title: Signable single-file packed executables (macOS Mach-O segment, Windows PE resource)
description: The pack trailer is appended after the runner's code signature, so a packed macOS binary fails codesign strict validation and Gatekeeper, and a signed Windows runner would lose Authenticode the same way; embed the pack inside the executable image (Mach-O segment, PE resource) and re-sign after packing.
created: 2026-08-18
---

# Signable single-file packed executables

`srt pack` (and `srt pack --flux` for fluxrt) produce a single-file
executable by appending the pack as a trailer to the runner image:
sections, section table, `[table offset][count][magic]`, located from EOF
by the runner (`packages/cli/src/packer.ts`, `lattice/src/main.rs`
`load_embedded_payload`, `flux/src/bin/fluxrt.rs`). That is a fine
single-file format on Linux, where nothing signs executables. On the two
platforms with executable signing it breaks the signature by construction.

The single-file executable is the point of `srt pack`; a bundle/folder
shape is not an answer here (a non-single-file output is a separate,
optional mode).

## Symptom

macOS, verified 2026-08-17 on an arm64 mac mini (macOS 26.3.1, Xcode 26.4):

- `srt pack examples/hello-world/src/index.tsx -o hello` writes a 57 MB
  Mach-O. It runs when launched locally: the kernel validates code
  signatures per page against the CodeDirectory, and the trailer lies
  beyond `codeLimit` (the end of the signed region, which is where the
  signature blob starts), so no signed page changed and AMFI never looks
  at the trailing bytes.
- `codesign --verify --strict hello` reports "main executable failed
  strict validation": userspace `codesign` (and Gatekeeper's assessment,
  `spctl`) require the code signature to be the last thing in the file and
  `__LINKEDIT` to end at EOF. A quarantined copy (downloaded, AirDropped)
  is refused. Re-signing the trailered file with `codesign --force` is
  undocumented territory: whether it refuses or emits a signature Gatekeeper
  still rejects is whatever the current Xcode does, so it is not a design.

Windows, not yet observed because nothing is signed today: Authenticode
hashes the whole PE except the checksum field, the certificate-table
directory entry, and the certificate table itself. Appended data is
outside the PE's declared sections but inside the hashed range, so a
signed runner with a trailer fails `signtool verify`, and SmartScreen
treats it as unsigned. Signing after packing works only if the pack is
inside the image the signature covers.

## Shape

One principle for both: the pack becomes part of the executable image
the platform's signature covers, and signing is a step that runs *after*
packing. The trailer format (sections + table) stays as the payload
encoding; only where it lives and how the runner finds it changes per
platform. Linux keeps the EOF trailer.

### macOS: Mach-O segment

Writer (CLI, runs on a mac since the runner is host-platform):

1. Parse the runner's Mach-O header and load commands.
2. Drop `LC_CODE_SIGNATURE` and its blob; the ad-hoc signature the
   Makefile applied is void after surgery anyway.
3. Insert a new `LC_SEGMENT_64` (`__SRT`, one section `__pack`) holding
   the trailer bytes, placed before `__LINKEDIT` in both file and VM
   order; `codesign` requires `__LINKEDIT` to be last. Shift
   `__LINKEDIT`'s fileoff/vmaddr and patch every load command with
   offsets into it: `LC_SYMTAB`, `LC_DYSYMTAB`, `LC_DYLD_CHAINED_FIXUPS`,
   `LC_DYLD_EXPORTS_TRIE`, `LC_FUNCTION_STARTS`, `LC_DATA_IN_CODE`. The
   new load command needs header slack; ld64 leaves plenty on our
   binaries (verify, and fail loudly if not).
4. `codesign --force --sign - <out>` (part of the OS, not Xcode). The
   result is a normal, valid Mach-O; users wanting Developer ID +
   notarization re-sign afterwards with their identity.

This is what Node's `postject`/LIEF and Deno's `sui` crate do. Roughly
200-300 lines of TypeScript, no dependency.

Reader (lattice + fluxrt, macOS): locate the `__SRT,__pack` section of
the running image instead of scanning from EOF. Prefer parsing
`current_exe()` with the `object` crate if it is already in the tree, or
the few structs by hand (header, `LC_SEGMENT_64`, section list): pure
Rust, keeps the plain-file-I/O model. `getsectiondata()` from
`<mach-o/getsect.h>` would be self-authored FFI, which we avoid.

### Windows: PE resource (or section)

Writer: embed the trailer bytes as an `RT_RCDATA` resource (or a dedicated
section, `.srtpack`) in the runner PE, updating the section table, image
size, and checksum. Signing is then `signtool sign` after packing, or left
to the user. Windows has no ad-hoc signing, so `srt pack` itself signs
nothing; the deliverable is that a signed output is *possible*. Reader:
locate the resource/section from the module's own image
(`FindResource`/`LoadResource` through the `windows-sys` crate, or parse
the PE section table from `current_exe()`).

Same encoding, same runner-side section reader; only the locator differs.

### Out of scope

- Developer ID / notarization / EV-cert workflow docs (follows once the
  container is signable).
- The non-single-file pack output mode.
- Android/Linux (unaffected; Linux keeps the trailer).

## Done looks like

- macOS: `srt pack` output passes `codesign --verify --strict` and
  `spctl --assess --type execute` after ad-hoc signing, launches from a
  quarantined copy, and runs (window up, `GPU ready`). Same for
  `srt pack --flux` (fluxrt).
- Windows: `srt pack` output can be `signtool sign`ed and then passes
  `signtool verify /pa`; the runner still finds its pack.
- Linux unchanged; the trailer reader stays for it.
- The `packer.ts` / `main.rs` / `fluxrt.rs` comments describe the
  per-platform container so the next reader does not rediscover this.

## Related

- Packed GL libraries (trailer kind 3): on macOS the runner now points
  SDL at the extracted ANGLE dylibs through `SDL_EGL_LIBRARY` /
  `SDL_OPENGL_LIBRARY` (`lattice/src/gl_libs.rs`), and Impeller is linked
  statically on macOS (`alloy/Cargo.toml`), so a packed executable is
  self-contained apart from the signature problem above.
- `okf/backlog/app-icons.md` stage 3 (packed-app icons) touches the same
  container: an icon in a Mach-O/PE lives in the app bundle plist or PE
  resources respectively, so it should ride on the same writer.
