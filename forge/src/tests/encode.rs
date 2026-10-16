// The encoders against the decoders beside them: a synthetic sequence
// through libvpx VP9 and back, a sine through libopus and back, and the
// RGBA to I420 conversion on known colors and odd sizes.

use crate::audio::{OpusApplication, OpusDecoder, OpusEncoder};
use crate::video::{
  rgba_to_i420, EncodeMode, EncodedFrame, EncoderConfig, PixelLayout, VideoAu, VideoDecoder, VideoEncoder, Vp9Decoder,
  Vp9Encoder,
};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;
const FRAME_RATE: u32 = 30;
const FRAMES: usize = 30;
const FRAME_US: i64 = 1_000_000 / FRAME_RATE as i64;
// Generous for 160x120: the round trip is judged on fidelity, not size.
const BITRATE_BPS: u32 = 400_000;
const KEYFRAME_INTERVAL_US: i64 = 2_000_000;
const FORCED_KEYFRAME: usize = 15;
// Luma fidelity floor over the sequence: well below what the bitrate
// yields on this content (around 40 dB), well above a broken plane.
const MIN_PSNR_DB: f64 = 30.0;
// BT.601 studio range: what white and black become.
const Y_WHITE: u8 = 235;
const Y_BLACK: u8 = 16;
const CHROMA_NEUTRAL: u8 = 128;

const OPUS_RATE: u32 = 16_000;
const OPUS_BITRATE_BPS: u32 = 32_000;
const OPUS_FRAMES: usize = 50;
const SINE_HZ: f32 = 440.0;
const SINE_PEAK: f32 = 0.5;
// Normalized correlation between the input and the decoded output once
// the codec delay is skipped: a lossy codec keeps a sine's shape well
// above this; a wrong delay or a broken frame falls far below.
const MIN_CORRELATION: f32 = 0.9;

fn config(mode: EncodeMode) -> EncoderConfig {
  EncoderConfig {
    width: WIDTH,
    height: HEIGHT,
    frame_rate: FRAME_RATE,
    bitrate_bps: BITRATE_BPS,
    keyframe_interval_us: KEYFRAME_INTERVAL_US,
    mode,
  }
}

// A gradient that slides one pixel per frame: every frame differs, every
// block has detail.
fn source_frame(index: usize) -> Vec<u8> {
  let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
  for y in 0..HEIGHT {
    for x in 0..WIDTH {
      rgba.extend_from_slice(&[((x as usize + index) % 256) as u8, (y * 2) as u8, 128, 255]);
    }
  }
  rgba
}

fn luma_psnr(a: &[u8], b: &[u8]) -> f64 {
  let n = (WIDTH * HEIGHT) as usize;
  let mse = a[..n].iter().zip(&b[..n]).map(|(&p, &q)| (p as f64 - q as f64).powi(2)).sum::<f64>() / n as f64;
  if mse == 0.0 {
    return f64::INFINITY;
  }
  10.0 * (255.0f64 * 255.0 / mse).log10()
}

fn encode_sequence(mode: EncodeMode) -> (Vec<Vec<u8>>, Vec<EncodedFrame>) {
  let mut encoder = Vp9Encoder::new(&config(mode)).expect("create encoder");
  let mut sources = Vec::new();
  let mut packets = Vec::new();
  let mut i420 = Vec::new();
  for index in 0..FRAMES {
    rgba_to_i420(&source_frame(index), WIDTH, HEIGHT, &mut i420).expect("convert");
    let pts = index as i64 * FRAME_US;
    packets.extend(encoder.encode(&i420, pts, index == FORCED_KEYFRAME).expect("encode"));
    sources.push(i420.clone());
  }
  packets.extend(encoder.flush().expect("flush"));
  (sources, packets)
}

#[test]
fn vp9_round_trips_through_the_decoder() {
  let (sources, packets) = encode_sequence(EncodeMode::Recording);
  assert_eq!(packets.len(), FRAMES, "one packet per frame, no hidden frames");
  assert!(packets[0].keyframe, "the first frame is a keyframe");
  assert!(packets[FORCED_KEYFRAME].keyframe, "a forced keyframe comes out as one");
  let others = packets.iter().enumerate().filter(|(i, p)| *i != 0 && *i != FORCED_KEYFRAME && p.keyframe).count();
  assert_eq!(others, 0, "no other keyframes inside the interval");
  for (index, packet) in packets.iter().enumerate() {
    assert_eq!(packet.pts_us, index as i64 * FRAME_US, "pts passes through the microsecond timebase");
    assert_eq!(packet.data[0] >> 6, 0b10, "every packet starts at a VP9 frame marker");
  }

  let mut decoder = Vp9Decoder::new(WIDTH, HEIGHT).expect("create decoder");
  let mut decoded = Vec::new();
  for packet in &packets {
    let au = VideoAu { pts_us: packet.pts_us, sync: packet.keyframe, data: packet.data.clone() };
    decoded.extend(decoder.decode(&au).expect("decode"));
  }
  decoded.extend(decoder.flush().expect("flush decoder"));
  assert_eq!(decoded.len(), FRAMES);
  for (frame, source) in decoded.iter().zip(&sources) {
    assert_eq!((frame.width, frame.height, frame.layout), (WIDTH, HEIGHT, PixelLayout::I420));
    let psnr = luma_psnr(&frame.data, source);
    assert!(psnr >= MIN_PSNR_DB, "frame at {} us: luma PSNR {psnr:.1} dB", frame.pts_us);
  }
}

#[test]
fn live_mode_encodes_and_changes_bitrate() {
  let mut encoder = Vp9Encoder::new(&config(EncodeMode::Live)).expect("create live encoder");
  let mut i420 = Vec::new();
  rgba_to_i420(&source_frame(0), WIDTH, HEIGHT, &mut i420).expect("convert");
  let first = encoder.encode(&i420, 0, false).expect("encode");
  assert_eq!(first.len(), 1);
  assert!(first[0].keyframe);
  encoder.set_bitrate(BITRATE_BPS / 4).expect("lower the bitrate");
  rgba_to_i420(&source_frame(1), WIDTH, HEIGHT, &mut i420).expect("convert");
  let second = encoder.encode(&i420, FRAME_US, false).expect("encode after the change");
  assert_eq!(second.len(), 1);
  assert!(!second[0].keyframe);
}

#[test]
fn encoder_refuses_a_frame_of_the_wrong_size() {
  let mut encoder = Vp9Encoder::new(&config(EncodeMode::Recording)).expect("create encoder");
  let err = encoder.encode(&[0; 16], 0, false).expect_err("a short frame");
  assert!(err.contains("I420"), "{err}");
  assert!(Vp9Encoder::new(&EncoderConfig { width: 0, ..config(EncodeMode::Recording) }).is_err());
}

#[test]
fn rgba_to_i420_studio_range_and_odd_sizes() {
  let mut out = Vec::new();
  let white = vec![255u8; 3 * 3 * 4];
  rgba_to_i420(&white, 3, 3, &mut out).expect("convert");
  assert_eq!(out.len(), 9 + 2 * 4, "odd sizes round chroma up");
  assert!(out[..9].iter().all(|&y| y == Y_WHITE));
  assert!(out[9..].iter().all(|&c| c == CHROMA_NEUTRAL));

  let black = vec![0u8; 2 * 2 * 4];
  rgba_to_i420(&black, 2, 2, &mut out).expect("convert");
  assert_eq!(out, vec![Y_BLACK, Y_BLACK, Y_BLACK, Y_BLACK, CHROMA_NEUTRAL, CHROMA_NEUTRAL]);

  // Pure blue: positive Cb, negative Cr, in the studio range.
  let blue: Vec<u8> = [0u8, 0, 255, 255].repeat(4);
  rgba_to_i420(&blue, 2, 2, &mut out).expect("convert");
  assert!(out[4] > CHROMA_NEUTRAL && out[4] <= 240, "Cb {}", out[4]);
  assert!(out[5] < CHROMA_NEUTRAL && out[5] >= 16, "Cr {}", out[5]);

  let err = rgba_to_i420(&white, 4, 4, &mut out).expect_err("size mismatch");
  assert!(err.contains("RGBA"), "{err}");
}

#[test]
fn opus_round_trips_through_the_decoder() {
  let mut encoder = OpusEncoder::new(OPUS_RATE, 1, OPUS_BITRATE_BPS, OpusApplication::Audio).expect("create encoder");
  let frame = encoder.frame_samples();
  assert_eq!(frame, 320, "20 ms at 16 kHz");
  let pre_skip = encoder.pre_skip();
  assert!(pre_skip > 0 && pre_skip < 48_000 / 10, "pre-skip {pre_skip} is a few ms at 48 kHz");

  let input: Vec<f32> = (0..frame * OPUS_FRAMES)
    .map(|i| SINE_PEAK * (2.0 * std::f32::consts::PI * SINE_HZ * i as f32 / OPUS_RATE as f32).sin())
    .collect();
  let mut decoder = OpusDecoder::new(OPUS_RATE, 1).expect("create decoder");
  let mut output = Vec::new();
  for (index, pcm) in input.chunks_exact(frame).enumerate() {
    let packet = encoder.encode(pcm).expect("encode");
    assert!(!packet.is_empty() && packet.len() < 4000);
    let chunk = decoder.decode(index as i64 * 20_000, &packet).expect("decode");
    assert_eq!(chunk.samples.len(), frame, "a 20 ms packet decodes to one frame");
    output.extend(chunk.samples);
  }

  // The decoder's output lags the input by the codec delay (pre-skip, at
  // the stream's own rate here).
  let delay = (pre_skip as usize * OPUS_RATE as usize) / 48_000;
  let n = input.len() - delay;
  let (mut dot, mut aa, mut bb) = (0.0f32, 0.0f32, 0.0f32);
  for i in 0..n {
    let (a, b) = (input[i], output[i + delay]);
    dot += a * b;
    aa += a * a;
    bb += b * b;
  }
  let correlation = dot / (aa.sqrt() * bb.sqrt());
  assert!(correlation >= MIN_CORRELATION, "correlation {correlation:.3} at delay {delay}");
}

#[test]
fn opus_encoder_refuses_what_libopus_does_not_take() {
  let err = OpusEncoder::new(44_100, 1, OPUS_BITRATE_BPS, OpusApplication::Voip).err().expect("44.1 kHz is refused");
  assert!(err.contains("48 kHz"), "{err}");
  let mut encoder = OpusEncoder::new(OPUS_RATE, 1, OPUS_BITRATE_BPS, OpusApplication::Voip).expect("create");
  let err = encoder.encode(&[0.0; 100]).expect_err("a partial frame");
  assert!(err.contains("per channel"), "{err}");
}
