# Test fixtures

Files the forge tests read from disk. This file records the origin and
license of every fixture that was not made in this repository.

## khronos_uastc_zstd.ktx2

A KTX2 file written by another tool, which pins the container parsing and
the zstd path of `forge::ktx2` against a writer that is not ours
(`src/tests/ktx2.rs`, `transcodes_a_file_from_another_writer`).

| | |
|---|---|
| Source | [KhronosGroup/KTX-Software](https://github.com/KhronosGroup/KTX-Software), `tests/resources/ktx2/ktx_document_uastc_rdo_4_zstd_5.ktx2` |
| Fetched | 2026-09-29, from the `main` branch |
| SHA-256 | `15913638d6d882c41bde2021443d1f0a83de29adb294dcb835fd4618baf19780` |
| Copyright | 2015-2022 The Khronos Group Inc. |
| License | Apache-2.0, as declared in that repository's `REUSE.toml` |
| Changes | None. Renamed only. |

Content: 1024x1024, 11 mip levels, UASTC with RDO and zstd
supercompression, written by `ktx create` of libktx 5.0.

The license text is `LICENSE-Apache-2.0.txt` in this directory, the
unmodified text from https://www.apache.org/licenses/LICENSE-2.0.txt. It
covers this fixture only; the repository itself is under the license in
its root `LICENSE`. The source repository ships no separate NOTICE file.

This file is test data. It is not compiled into or distributed with any
binary.
