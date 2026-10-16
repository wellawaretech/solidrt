// The writer against the reader beside it: a clip of encoded VP9 and Opus
// written as a file reads back with its facts, its blocks and its seeks;
// the same clip written as a stream reads back as unbounded; an audio-only
// file decodes as a clip through forge::audio.

use std::io::Cursor;

use super::sink::SharedSink;
use crate::audio::{self, OpusApplication, OpusEncoder};
use crate::source::Facts;
use crate::video::{
  rgba_to_i420, Demuxer, EncodeMode, EncodedFrame, EncoderConfig, ErrorKind, Packet, VideoEncoder, Vp9Encoder,
  WebmAudioTrack, WebmDemuxer, WebmSink, WebmVideoTrack, WebmWriter,
};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;
const FRAME_RATE: u32 = 30;
const FRAMES: usize = 30;
const FRAME_US: i64 = 1_000_000 / FRAME_RATE as i64;
const FORCED_KEYFRAME: usize = 15;
const VIDEO_BITRATE_BPS: u32 = 400_000;
const KEYFRAME_INTERVAL_US: i64 = 2_000_000;
const OPUS_RATE: u32 = 16_000;
const OPUS_BITRATE_BPS: u32 = 32_000;
const OPUS_PACKET_US: i64 = 20_000;
const OPUS_PACKETS: usize = 50;
const SINE_HZ: f32 = 440.0;
const SINE_PEAK: f32 = 0.5;
// The clip is one second of both tracks; the Duration adds one frame
// length after the last block, so a few ms either way.
const CLIP_US: i64 = 1_000_000;
const DURATION_TOLERANCE_US: i64 = 40_000;
// The Opus seek preroll the writer declares, in microseconds.
const SEEK_PREROLL_US: i64 = 80_000;
const EBML_MAGIC: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];

fn video_frames() -> Vec<EncodedFrame> {
  let config = EncoderConfig {
    width: WIDTH,
    height: HEIGHT,
    frame_rate: FRAME_RATE,
    bitrate_bps: VIDEO_BITRATE_BPS,
    keyframe_interval_us: KEYFRAME_INTERVAL_US,
    mode: EncodeMode::Recording,
  };
  let mut encoder = Vp9Encoder::new(&config).expect("create encoder");
  let mut frames = Vec::new();
  let mut i420 = Vec::new();
  for index in 0..FRAMES {
    let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
      for x in 0..WIDTH {
        rgba.extend_from_slice(&[((x as usize + index) % 256) as u8, (y * 2) as u8, 128, 255]);
      }
    }
    rgba_to_i420(&rgba, WIDTH, HEIGHT, &mut i420).expect("convert");
    frames.extend(encoder.encode(&i420, index as i64 * FRAME_US, index == FORCED_KEYFRAME).expect("encode"));
  }
  frames.extend(encoder.flush().expect("flush"));
  frames
}

/// Opus packets of a sine, with the encoder's pre-skip.
fn audio_packets() -> (u32, Vec<(i64, Vec<u8>)>) {
  let mut encoder = OpusEncoder::new(OPUS_RATE, 1, OPUS_BITRATE_BPS, OpusApplication::Audio).expect("create encoder");
  let frame = encoder.frame_samples();
  let input: Vec<f32> = (0..frame * OPUS_PACKETS)
    .map(|i| SINE_PEAK * (2.0 * std::f32::consts::PI * SINE_HZ * i as f32 / OPUS_RATE as f32).sin())
    .collect();
  let packets = input
    .chunks_exact(frame)
    .enumerate()
    .map(|(index, pcm)| (index as i64 * OPUS_PACKET_US, encoder.encode(pcm).expect("encode")))
    .collect();
  (encoder.pre_skip(), packets)
}

/// Write a clip of the chosen tracks, interleaved by time.
fn write_clip(sink: WebmSink, video: bool, audio: bool) -> u32 {
  let frames = if video { video_frames() } else { Vec::new() };
  let (pre_skip, packets) = if audio { audio_packets() } else { (0, Vec::new()) };
  let video_track = video.then_some(WebmVideoTrack { width: WIDTH, height: HEIGHT });
  let audio_track = audio.then_some(WebmAudioTrack { channels: 1, input_sample_rate: OPUS_RATE, pre_skip });
  let mut writer = WebmWriter::new(sink, video_track, audio_track).expect("create writer");
  let (mut f, mut p) = (0, 0);
  while f < frames.len() || p < packets.len() {
    let video_next = f < frames.len() && (p >= packets.len() || frames[f].pts_us <= packets[p].0);
    if video_next {
      writer.write_video(&frames[f]).expect("write video");
      f += 1;
    } else {
      writer.write_audio(packets[p].0, &packets[p].1).expect("write audio");
      p += 1;
    }
  }
  let duration = writer.duration_us();
  assert!((duration - CLIP_US).abs() <= DURATION_TOLERANCE_US, "writer duration {duration}");
  writer.finish().expect("finish");
  pre_skip
}

fn ms(us: i64) -> i64 {
  (us + 500) / 1000 * 1000
}

fn read_all(demux: &mut WebmDemuxer) -> (Vec<(i64, bool)>, Vec<i64>) {
  let (mut video, mut audio) = (Vec::new(), Vec::new());
  while let Some(packet) = demux.next_packet().expect("read") {
    match packet {
      Packet::Video(au) => video.push((au.pts_us, au.sync)),
      Packet::Audio(packet) => audio.push(packet.pts_us),
    }
  }
  (video, audio)
}

fn assert_blocks(demux: &mut WebmDemuxer) {
  let (video, audio) = read_all(demux);
  assert_eq!(video.len(), FRAMES);
  assert_eq!(audio.len(), OPUS_PACKETS);
  for (index, &(pts, sync)) in video.iter().enumerate() {
    assert_eq!(pts, ms(index as i64 * FRAME_US), "video pts rounded to the millisecond tick");
    assert_eq!(sync, index == 0 || index == FORCED_KEYFRAME, "keyframe flag of frame {index}");
  }
  for (index, &pts) in audio.iter().enumerate() {
    assert_eq!(pts, index as i64 * OPUS_PACKET_US);
  }
}

#[test]
fn file_reads_back_with_its_facts_and_seeks() {
  let sink = SharedSink::default();
  let pre_skip = write_clip(WebmSink::File(Box::new(sink.clone())), true, true);
  let bytes = sink.bytes();
  assert_eq!(bytes[..4], EBML_MAGIC);

  let len = bytes.len() as u64;
  let mut demux =
    WebmDemuxer::open(Box::new(Cursor::new(bytes)), Facts { len: Some(len), seekable: true }).expect("open");
  let info = demux.info().clone();
  assert_eq!((info.width, info.height), (WIDTH, HEIGHT));
  let duration = info.duration_us.expect("a file has a duration");
  assert!((duration - CLIP_US).abs() <= DURATION_TOLERANCE_US, "duration {duration}");
  assert!(info.seekable);
  assert!(!info.bt709, "BT.601 is written into Colour");
  assert!(!info.full_range, "studio range is written into Colour");
  let audio = info.audio.expect("audio track");
  assert_eq!((audio.sample_rate, audio.channels), (48_000, 1));
  assert_eq!(audio.pre_skip, pre_skip);
  assert_eq!(audio.seek_preroll_us, SEEK_PREROLL_US);
  assert_blocks(&mut demux);

  // Cues: one per cluster, a cluster per keyframe. A seek past the forced
  // keyframe lands on its cluster, resumes at the target, and the audio
  // starts a preroll before it.
  let target = FORCED_KEYFRAME as i64 * FRAME_US + 200_000;
  let resume = demux.seek(target).expect("seek");
  assert_eq!(resume, target);
  let first = demux.next_video().expect("read").expect("a frame after the seek");
  assert_eq!((first.pts_us, first.sync), (ms(FORCED_KEYFRAME as i64 * FRAME_US), true));
  let first_audio = demux.next_audio().expect("read").expect("audio after the seek");
  // The first packet at or after the preroll point (a ceiling).
  let preroll_from = target - SEEK_PREROLL_US;
  assert_eq!(first_audio.pts_us, (preroll_from + OPUS_PACKET_US - 1) / OPUS_PACKET_US * OPUS_PACKET_US);

  // Back to the start: the first cue.
  assert_eq!(demux.seek(0).expect("seek to 0"), 0);
  let (video, audio) = read_all(&mut demux);
  assert_eq!((video.len(), audio.len()), (FRAMES, OPUS_PACKETS));
}

#[test]
fn stream_reads_back_as_unbounded() {
  let sink = SharedSink::default();
  write_clip(WebmSink::Stream(Box::new(sink.clone())), true, true);
  let bytes = sink.bytes();
  let mut demux = WebmDemuxer::open(Box::new(Cursor::new(bytes)), Facts { len: None, seekable: false }).expect("open");
  assert_eq!(demux.info().duration_us, None, "an unknown-size segment has no duration");
  assert!(!demux.info().seekable);
  assert_blocks(&mut demux);
  let err = demux.seek(0).expect_err("a stream cannot seek");
  assert_eq!(err.kind, ErrorKind::Unsupported);
}

#[test]
fn video_only_and_audio_only_files_open() {
  let sink = SharedSink::default();
  write_clip(WebmSink::File(Box::new(sink.clone())), true, false);
  let bytes = sink.bytes();
  let len = bytes.len() as u64;
  let mut demux =
    WebmDemuxer::open(Box::new(Cursor::new(bytes)), Facts { len: Some(len), seekable: true }).expect("open video only");
  assert!(demux.info().audio.is_none());
  let (video, audio) = read_all(&mut demux);
  assert_eq!((video.len(), audio.len()), (FRAMES, 0));

  // Audio alone: the clip decoder path (forge::audio over the reader),
  // which trims the pre-skip and yields the sine.
  let sink = SharedSink::default();
  write_clip(WebmSink::File(Box::new(sink.clone())), false, true);
  let mut decoder = audio::open(Box::new(Cursor::new(sink.bytes()))).expect("open audio only");
  let format = decoder.format();
  assert_eq!((format.sample_rate, format.channels), (48_000, 1));
  let frames = format.frames.expect("a file has a length");
  assert!((frames as i64 - 48_000).abs() <= DURATION_TOLERANCE_US * 48 / 1000, "frames {frames}");
  let mut samples = Vec::new();
  let mut chunk = vec![0.0f32; 1000];
  loop {
    let n = decoder.read(&mut chunk).expect("read");
    if n == 0 {
      break;
    }
    samples.extend_from_slice(&chunk[..n]);
  }
  assert!((samples.len() as i64 - 48_000).abs() <= DURATION_TOLERANCE_US * 48 / 1000, "{} samples", samples.len());
  let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
  assert!((peak - SINE_PEAK).abs() < SINE_PEAK * 0.2, "peak {peak}");
}

#[test]
fn writer_refuses_an_empty_track_set_and_the_wrong_track() {
  let err = WebmWriter::new(WebmSink::Stream(Box::new(Vec::new())), None, None).err().expect("no tracks");
  assert!(err.contains("no tracks"), "{err}");
  let mut writer = WebmWriter::new(
    WebmSink::Stream(Box::new(Vec::new())),
    None,
    Some(WebmAudioTrack { channels: 1, input_sample_rate: OPUS_RATE, pre_skip: 0 }),
  )
  .expect("audio only");
  let err =
    writer.write_video(&EncodedFrame { pts_us: 0, keyframe: true, data: vec![0x82] }).expect_err("no video track");
  assert!(err.contains("no video track"), "{err}");
}
