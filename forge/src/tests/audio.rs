use std::io::Cursor;

use crate::audio::{self, AudioDecoder, AudioFormat};

// The fixtures (src/tests/data/README.md): ffmpeg's sine source, 440 Hz at
// amplitude 1/8, as 0.1 s at 8 kHz in each WAV layout and as 0.5 s at
// 48 kHz in Opus.
const SINE_HZ: f64 = 440.0;
const SINE_PEAK: f32 = 0.125;
const WAV_RATE: u32 = 8000;
const WAV_FRAMES: u64 = 800;
const OPUS_RATE: u32 = 48_000;
const OPUS_FRAMES: u64 = 24_000;
// Zero crossings a sine has over a duration: two per cycle. The tolerance
// covers the phase at the ends and, for Opus, the first converging samples.
const CROSSING_TOLERANCE: usize = 6;
// A quantization step for each integer layout, as the tolerance against the
// float layout (the fixtures were all converted from one s16 source).
const U8_STEP: f32 = 1.0 / 128.0;
const S16_STEP: f32 = 1.0 / 32768.0;
const EXACT: f32 = 1e-6;

fn fixture(name: &str) -> Vec<u8> {
  std::fs::read(format!("{}/src/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).expect("read fixture")
}

fn open(bytes: Vec<u8>) -> Box<dyn AudioDecoder> {
  audio::open(Box::new(Cursor::new(bytes))).expect("open clip")
}

fn open_error(bytes: Vec<u8>) -> String {
  match audio::open(Box::new(Cursor::new(bytes))) {
    Ok(_) => panic!("open must fail"),
    Err(e) => e,
  }
}

// Read to the end in uneven chunks, so partial fills are exercised.
fn read_all(decoder: &mut dyn AudioDecoder) -> Vec<f32> {
  let channels = decoder.format().channels as usize;
  let mut out = Vec::new();
  let mut chunk = vec![0.0f32; 333 * channels + 1];
  loop {
    let frames = decoder.read(&mut chunk).expect("read");
    if frames == 0 {
      return out;
    }
    out.extend_from_slice(&chunk[..frames * channels]);
  }
}

fn zero_crossings(samples: &[f32]) -> usize {
  samples.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count()
}

fn peak(samples: &[f32]) -> f32 {
  samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

fn assert_sine(samples: &[f32], rate: u32, what: &str) {
  let seconds = samples.len() as f64 / rate as f64;
  let expected = (2.0 * SINE_HZ * seconds).round() as usize;
  let crossings = zero_crossings(samples);
  assert!(
    crossings.abs_diff(expected) <= CROSSING_TOLERANCE,
    "{what}: {crossings} zero crossings, expected ~{expected}"
  );
  let p = peak(samples);
  assert!((p - SINE_PEAK).abs() < SINE_PEAK * 0.2, "{what}: peak {p}, expected ~{SINE_PEAK}");
}

fn assert_close(a: &[f32], b: &[f32], tolerance: f32, what: &str) {
  assert_eq!(a.len(), b.len(), "{what}: length");
  for (i, (x, y)) in a.iter().zip(b).enumerate() {
    assert!((x - y).abs() <= tolerance, "{what}: sample {i} is {x}, reference {y}");
  }
}

#[test]
fn wav_float_layout_is_the_sine() {
  let mut decoder = open(fixture("audio_f32.wav"));
  assert_eq!(decoder.format(), AudioFormat { sample_rate: WAV_RATE, channels: 1, frames: Some(WAV_FRAMES) });
  let samples = read_all(decoder.as_mut());
  assert_eq!(samples.len() as u64, WAV_FRAMES);
  assert_sine(&samples, WAV_RATE, "f32");
}

#[test]
fn wav_integer_layouts_decode_to_the_same_signal() {
  let reference = read_all(open(fixture("audio_f32.wav")).as_mut());
  for (name, tolerance) in
    [("audio_u8.wav", U8_STEP), ("audio_s16.wav", S16_STEP), ("audio_s24.wav", EXACT), ("audio_s32.wav", EXACT)]
  {
    let mut decoder = open(fixture(name));
    assert_eq!(decoder.format().frames, Some(WAV_FRAMES), "{name}");
    let samples = read_all(decoder.as_mut());
    assert_close(&samples, &reference, tolerance, name);
  }
}

#[test]
fn wav_stereo_interleaves_two_channels() {
  let reference = read_all(open(fixture("audio_s16.wav")).as_mut());
  let mut decoder = open(fixture("audio_s16_stereo.wav"));
  assert_eq!(decoder.format(), AudioFormat { sample_rate: WAV_RATE, channels: 2, frames: Some(WAV_FRAMES) });
  let samples = read_all(decoder.as_mut());
  assert_eq!(samples.len() as u64, WAV_FRAMES * 2);
  let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
  let right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
  assert_close(&left, &reference, EXACT, "left");
  assert_close(&right, &reference, EXACT, "right");
}

#[test]
fn wav_seeks_to_a_frame() {
  let whole = read_all(open(fixture("audio_s16.wav")).as_mut());
  let mut decoder = open(fixture("audio_s16.wav"));
  decoder.seek(500).expect("seek");
  let tail = read_all(decoder.as_mut());
  assert_eq!(tail.len(), 300);
  assert_close(&tail, &whole[500..], EXACT, "tail after seek");
  decoder.seek(10_000).expect("seek past the end clamps");
  assert_eq!(read_all(decoder.as_mut()).len(), 0);
  decoder.seek(0).expect("seek back");
  assert_eq!(read_all(decoder.as_mut()).len() as u64, WAV_FRAMES);
}

// A WAV assembled by hand: `chunks` in order after the RIFF header, the
// RIFF size computed. Each chunk is (id, payload); odd payloads get the
// pad byte a writer adds.
fn wav_bytes(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
  let mut body = b"WAVE".to_vec();
  for (id, payload) in chunks {
    body.extend_from_slice(*id);
    body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    body.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
      body.push(0);
    }
  }
  let mut bytes = b"RIFF".to_vec();
  bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
  bytes.extend_from_slice(&body);
  bytes
}

fn fmt_chunk(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
  let block_align = channels * bits / 8;
  let mut fmt = Vec::new();
  fmt.extend_from_slice(&tag.to_le_bytes());
  fmt.extend_from_slice(&channels.to_le_bytes());
  fmt.extend_from_slice(&rate.to_le_bytes());
  fmt.extend_from_slice(&(rate * block_align as u32).to_le_bytes());
  fmt.extend_from_slice(&block_align.to_le_bytes());
  fmt.extend_from_slice(&bits.to_le_bytes());
  fmt
}

fn s16_data(values: &[i16]) -> Vec<u8> {
  values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

#[test]
fn wav_walks_chunks_and_pads() {
  // JUNK (odd, padded) before fmt, a LIST between fmt and data, a trailing
  // chunk after data: all skipped, the samples come out as written.
  let bytes = wav_bytes(&[
    (b"JUNK", vec![0; 3]),
    (b"fmt ", fmt_chunk(1, 1, 8000, 16)),
    (b"LIST", b"INFOISFT\x04\x00\x00\x00test".to_vec()),
    (b"data", s16_data(&[0, 16384, -16384, 32767])),
    (b"cue ", vec![0; 4]),
  ]);
  let pcm = audio::decode(&bytes).expect("decode");
  assert_eq!((pcm.sample_rate, pcm.channels, pcm.frames()), (8000, 1, 4));
  assert_close(&pcm.samples, &[0.0, 0.5, -0.5, 32767.0 / 32768.0], EXACT, "samples");
}

#[test]
fn wav_truncated_data_plays_what_is_there() {
  let mut bytes = wav_bytes(&[(b"fmt ", fmt_chunk(1, 1, 8000, 16)), (b"data", s16_data(&[1000, 2000, 3000, 4000]))]);
  // The file ends mid-data: three whole frames and a stray byte remain.
  bytes.truncate(bytes.len() - 3);
  let mut decoder = open(bytes);
  assert_eq!(decoder.format().frames, Some(2));
  assert_eq!(read_all(decoder.as_mut()).len(), 2);
  // A streaming writer's unknown data size runs to the end of the file.
  let mut streamed = wav_bytes(&[(b"fmt ", fmt_chunk(1, 1, 8000, 16)), (b"data", s16_data(&[1, 2, 3]))]);
  let data_size_at = streamed.len() - 3 * 2 - 4;
  streamed[data_size_at..data_size_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
  assert_eq!(open(streamed).format().frames, Some(3));
}

#[test]
fn wav_rejects_what_it_cannot_play_by_name() {
  assert!(open_error(fixture("audio_adpcm.wav")).contains("MS ADPCM"));
  assert!(open_error(fixture("audio_rf64.wav")).contains("RF64"));
  let three = wav_bytes(&[(b"fmt ", fmt_chunk(1, 3, 8000, 16)), (b"data", vec![0; 6])]);
  assert!(open_error(three).contains("3 channels"));
  let law = wav_bytes(&[(b"fmt ", fmt_chunk(7, 1, 8000, 8)), (b"data", vec![0; 6])]);
  assert!(open_error(law).contains("mu-law"));
  let no_data = wav_bytes(&[(b"fmt ", fmt_chunk(1, 1, 8000, 16))]);
  assert!(open_error(no_data).contains("no data chunk"));
  let no_fmt = wav_bytes(&[(b"data", vec![0; 4])]);
  assert!(open_error(no_fmt).contains("before the fmt"));
}

#[test]
fn other_bytes_are_named() {
  assert!(open_error(b"hello world, not audio".to_vec()).contains("unrecognized"));
  assert!(open_error(fixture("audio_vorbis.ogg")).contains("Vorbis"));
  assert!(open_error(Vec::new()).contains("unrecognized"));
}

#[test]
fn opus_file_decodes_seeks_and_reports_its_length() {
  let mut decoder = open(fixture("audio_sine.opus"));
  assert_eq!(decoder.format(), AudioFormat { sample_rate: OPUS_RATE, channels: 1, frames: Some(OPUS_FRAMES) });
  let samples = read_all(decoder.as_mut());
  assert_eq!(samples.len() as u64, OPUS_FRAMES, "pre-skip and end trim are the library's");
  assert_sine(&samples, OPUS_RATE, "opus");
  decoder.seek(OPUS_FRAMES / 2).expect("seek");
  let tail = read_all(decoder.as_mut());
  assert_eq!(tail.len() as u64, OPUS_FRAMES / 2);
  assert_sine(&tail, OPUS_RATE, "opus tail");
  decoder.seek(0).expect("seek back");
  assert_eq!(read_all(decoder.as_mut()).len() as u64, OPUS_FRAMES);
}

#[test]
fn decode_reads_a_whole_clip() {
  let pcm = audio::decode(&fixture("audio_sine.opus")).expect("decode");
  assert_eq!((pcm.sample_rate, pcm.channels, pcm.frames() as u64), (OPUS_RATE, 1, OPUS_FRAMES));
}

#[cfg(feature = "video")]
#[test]
fn webm_audio_decodes_and_seeks() {
  // A WebM's Duration is the muxer's statement, rounded to its timestamp
  // scale (a millisecond), so the declared length is near the half second;
  // the decoded length is exact: the packets less the pre-skip at the
  // start and the DiscardPadding the last packet carries.
  const DECLARED_TOLERANCE_FRAMES: u64 = 480;
  let mut decoder = open(fixture("audio_sine.webm"));
  let format = decoder.format();
  assert_eq!((format.sample_rate, format.channels), (OPUS_RATE, 1));
  let declared = format.frames.expect("the container states a duration");
  assert!(declared.abs_diff(OPUS_FRAMES) <= DECLARED_TOLERANCE_FRAMES, "declared {declared}");
  let samples = read_all(decoder.as_mut());
  assert_eq!(samples.len() as u64, OPUS_FRAMES, "pre-skip and discard padding trimmed");
  assert_sine(&samples, OPUS_RATE, "webm");
  // A seek lands on the target through the preroll trim, to within the
  // container's timestamp precision: ffmpeg writes Opus blocks on a 1 ms
  // scale and rounds twice (the packet time, then the 6.5 ms CodecDelay),
  // so the file's own timestamps are up to one tick off the samples.
  const TIMESTAMP_TICK_FRAMES: u64 = 48;
  decoder.seek(OPUS_FRAMES / 2).expect("seek");
  let tail = read_all(decoder.as_mut());
  assert!((tail.len() as u64).abs_diff(OPUS_FRAMES / 2) <= TIMESTAMP_TICK_FRAMES, "tail {}", tail.len());
  assert_sine(&tail, OPUS_RATE, "webm tail");
}

#[cfg(feature = "video")]
#[test]
fn webm_without_an_audio_track_is_refused() {
  // video_kf.webm has a video track only.
  let error = open_error(fixture("video_kf.webm"));
  assert!(error.contains("no Opus audio track"), "{error}");
}
