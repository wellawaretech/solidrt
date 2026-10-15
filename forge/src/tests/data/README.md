# Test fixtures

Files the forge tests read from disk. This file records the origin and
license of every fixture that was not made in this repository, and how
the generated ones were made.

## audio_* (generated)

`src/tests/audio.rs`. All from ffmpeg's `sine` source (440 Hz, amplitude
1/8), so every layout decodes to the same signal:

```
# one per layout: s16 pcm_s16le, s24 pcm_s24le, s32 pcm_s32le, f32 pcm_f32le, u8 pcm_u8
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=8000:duration=0.1" -c:a pcm_s16le audio_s16.wav
# both channels the mono signal (a plain -ac 2 upmix would scale it by -3 dB)
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=8000:duration=0.1" -af "pan=stereo|c0=c0|c1=c0" -c:a pcm_s16le audio_s16_stereo.wav
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=8000:duration=0.1" -c:a adpcm_ms audio_adpcm.wav
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=8000:duration=0.1" -c:a pcm_s16le -rf64 always audio_rf64.wav
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=0.5" -c:a libopus -b:a 64k audio_sine.opus
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=0.5" -c:a libopus -b:a 64k audio_sine.webm
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=0.5" -c:a libvorbis -q:a 2 audio_vorbis.ogg
```

ffmpeg writes s24 and s32 as WAVE_FORMAT_EXTENSIBLE, f32 as format tag 3,
the rest as plain PCM; the ADPCM, RF64 and Vorbis files exist to be refused.

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
