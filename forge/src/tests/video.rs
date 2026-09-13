use crate::video::reader::Next;
use crate::video::transport::AudioSink;
use crate::video::{
  AudioPacket, Demuxer, OpusDecoder, PixelLayout, VideoAu, VideoDecoder, VideoPlayer, WebmDemuxer, YuvFrame,
};

// 2 s of ffmpeg testsrc2 160x120 at 25 fps as VP9 (libvpx-vp9, profile 0)
// with a 440 Hz sine as mono Opus (20 ms packets), in WebM.
fn fixture() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_av.webm").to_string()
}

// The demuxer as an audio feeder's queue, for the track tests.
fn audio_from(demux: &mut WebmDemuxer) -> Next<AudioPacket> {
  match demux.next_audio() {
    Ok(Some(packet)) => Next::Packet(packet),
    Ok(None) => Next::End,
    Err(e) => panic!("read audio: {e}"),
  }
}

fn open_fixture() -> WebmDemuxer {
  WebmDemuxer::open_path(&fixture()).expect("open fixture")
}

// Opus packets in the fixture: 20 ms each, so 960 samples at 48 kHz.
const OPUS_PACKET_SAMPLES: usize = 960;
const FIXTURE_AUDIO_PACKETS: usize = 101;

#[test]
fn header_reports_the_stream_facts() {
  let demux = WebmDemuxer::open_path(&fixture()).expect("open fixture");
  let info = demux.info();
  assert_eq!((info.width, info.height), (160, 120));
  let duration_us = info.duration_us.expect("a file has a duration");
  assert!((duration_us - 2_000_000).abs() < 100_000, "duration {duration_us} not ~2s");
  let audio = info.audio.as_ref().expect("audio track");
  assert_eq!((audio.sample_rate, audio.channels), (48000, 1));
  // libopus primes with 6.5 ms (312 samples); ffmpeg writes the 80 ms
  // preroll the Opus spec asks for.
  assert_eq!(audio.pre_skip, 312);
  assert_eq!(audio.seek_preroll_us, 80_000);
  // ffmpeg writes no Colour for an unspecified matrix: the resolution
  // default applies, and the range defaults to studio.
  assert!(!demux.info().bt709, "SD content defaults to BT.601");
  assert!(!demux.info().full_range, "studio range by default");
}

#[test]
fn video_samples_are_raw_vp9_frames() {
  let mut demux = WebmDemuxer::open_path(&fixture()).expect("open fixture");
  let first = demux.next_video().expect("read").expect("first frame");
  assert!(first.sync, "first frame is the keyframe");
  assert_eq!(first.pts_us, 0);
  // A VP9 uncompressed header opens with frame_marker 0b10, then profile
  // bits 0,0 for profile 0 and show_existing_frame 0: the sample is the
  // coded frame itself, no container framing in front of it.
  assert_eq!(first.data[0] >> 4, 0b1000, "VP9 frame marker, profile 0, shown frame");

  let mut count = 1;
  let mut last_pts = first.pts_us;
  while let Some(au) = demux.next_video().expect("read") {
    assert!(au.pts_us > last_pts, "VP9 pts are monotonic (no reordered samples)");
    assert_eq!(au.pts_us - last_pts, 40_000, "25 fps spacing");
    assert_eq!(au.data[0] >> 6, 0b10, "every sample starts at a VP9 frame marker");
    last_pts = au.pts_us;
    count += 1;
  }
  assert_eq!(count, 50);
  assert!(demux.next_video().expect("read past end").is_none(), "stays exhausted");
}

#[test]
fn opus_decodes_every_packet_to_pcm() {
  let mut demux = WebmDemuxer::open_path(&fixture()).expect("open fixture");
  let info = demux.info().audio.clone().expect("audio track");
  let mut decoder = OpusDecoder::new(info.sample_rate, info.channels).expect("create opus decoder");
  let mut packets = 0;
  let mut pcm_frames = 0;
  let mut peak = 0.0f32;
  let mut last_pts = -1;
  while let Some(packet) = demux.next_audio().expect("read") {
    assert!(packet.pts_us > last_pts, "audio pts are monotonic");
    last_pts = packet.pts_us;
    let chunk = decoder.decode(packet.pts_us, &packet.data).expect("decode");
    assert_eq!((chunk.sample_rate, chunk.channels), (48000, 1));
    pcm_frames += chunk.samples.len() / chunk.channels as usize;
    peak = chunk.samples.iter().fold(peak, |p, &s| p.max(s.abs()));
    packets += 1;
  }
  assert_eq!(packets, FIXTURE_AUDIO_PACKETS);
  assert_eq!(pcm_frames, FIXTURE_AUDIO_PACKETS * OPUS_PACKET_SAMPLES, "20 ms packets");
  // A sine at default ffmpeg volume: clearly audible, never clipping.
  assert!(peak > 0.1 && peak <= 1.0, "peak {peak} out of range");
}

// A sink standing in for the platform's: what was pushed, consumed on
// demand by the test, so the feeder's trims and clock mapping can be read
// back exactly. The state is shared so the test keeps a hand on it after
// the track has taken the sink.
#[derive(Default)]
struct FakeSinkState {
  pushed_frames: usize,
  consumed_frames: usize,
  paused: bool,
}

impl FakeSinkState {
  fn consume(&mut self, frames: usize) {
    self.consumed_frames = self.consumed_frames.saturating_add(frames).min(self.pushed_frames);
  }
  fn queued_us(&self) -> i64 {
    (self.pushed_frames - self.consumed_frames) as i64 * 1_000_000 / 48_000
  }
}

struct FakeSink(std::sync::Arc<std::sync::Mutex<FakeSinkState>>);

impl AudioSink for FakeSink {
  fn push(&mut self, samples: &[f32]) -> Result<(), String> {
    self.0.lock().expect("sink state").pushed_frames += samples.len();
    Ok(())
  }
  fn queued_us(&self) -> i64 {
    self.0.lock().expect("sink state").queued_us()
  }
  fn position_us(&self) -> i64 {
    self.0.lock().expect("sink state").consumed_frames as i64 * 1_000_000 / 48_000
  }
  fn set_paused(&mut self, paused: bool) {
    self.0.lock().expect("sink state").paused = paused;
  }
  fn clear(&mut self) {
    let mut state = self.0.lock().expect("sink state");
    state.consumed_frames = state.pushed_frames;
  }
}

#[test]
fn audio_track_trims_pre_skip_and_maps_the_sink_position_to_content_time() {
  use crate::video::audio::{AudioTrack, AUDIO_LOOKAHEAD_US, AUDIO_OUTPUT_LATENCY_US};
  let mut demux = WebmDemuxer::open_path(&fixture()).expect("open fixture");
  let info = demux.info().audio.clone().expect("audio track");
  let state = std::sync::Arc::new(std::sync::Mutex::new(FakeSinkState::default()));
  let sink: Box<dyn AudioSink> = Box::new(FakeSink(state.clone()));
  let mut track = AudioTrack::new(&info, 0, sink).expect("create audio track");
  assert!(track.content_time_us().is_none(), "no clock before the first push");

  // One feed fills the lookahead and no more.
  track.feed(|| audio_from(&mut demux));
  let queued = state.lock().expect("state").queued_us();
  assert!(queued >= AUDIO_LOOKAHEAD_US && queued < AUDIO_LOOKAHEAD_US + 20_000, "queued {queued}us");
  // The first chunk lost its pre-skip: the sink starts at content time 0
  // (minus the output latency the mapping assumes) and the queue holds
  // exactly the content pushed. WebM keeps block times in whole
  // milliseconds while Opus packets start at 20 ms steps minus the 6.5 ms
  // pre-skip, so the mapping carries up to half a millisecond of rounding.
  let content = track.content_time_us().expect("a clock after the first push");
  assert!((content + AUDIO_OUTPUT_LATENCY_US).abs() <= 1_000, "content time {content}us");
  let pushed = state.lock().expect("state").pushed_frames;
  let packets = (pushed + info.pre_skip as usize).div_ceil(OPUS_PACKET_SAMPLES);
  assert_eq!(pushed, packets * OPUS_PACKET_SAMPLES - info.pre_skip as usize);

  // Consuming moves content time one for one.
  state.lock().expect("state").consume(48_000 / 10);
  let later = track.content_time_us().expect("a clock");
  assert_eq!(later - content, 100_000);

  // A seek clears the queue, and the preroll packets before the target
  // are decoded but not pushed: the first sample pushed is the target.
  demux.seek(1_000_000).expect("seek");
  track.seek(1_000_000);
  assert!(track.content_time_us().is_none());
  let before = state.lock().expect("state").pushed_frames;
  track.feed(|| audio_from(&mut demux));
  let content = track.content_time_us().expect("a clock after the seek's first push");
  assert!((content - (1_000_000 - AUDIO_OUTPUT_LATENCY_US)).abs() <= 1_000, "content time {content}us");

  // The end of the track: everything left drains, then feed is a no-op.
  loop {
    state.lock().expect("state").consume(usize::MAX);
    let pushed = state.lock().expect("state").pushed_frames;
    track.feed(|| audio_from(&mut demux));
    if state.lock().expect("state").pushed_frames == pushed {
      break;
    }
  }
  let total = state.lock().expect("state").pushed_frames - before;
  // From the 1 s target to the end of the 2.008 s track.
  let expected = FIXTURE_AUDIO_PACKETS * OPUS_PACKET_SAMPLES - 48_000;
  assert!((total as i64 - expected as i64).abs() <= OPUS_PACKET_SAMPLES as i64, "pushed {total}, expected ~{expected}");
}

// Stands in for the decoder in the player tests, so they exercise frame
// selection alone: one frame out per coded frame, at its own pts, mid-gray
// so plane sizes and ordering are still checked.
struct StubDecoder;

impl VideoDecoder for StubDecoder {
  fn decode(&mut self, au: &VideoAu) -> Result<Vec<YuvFrame>, String> {
    let layout = crate::video::decoded_layout();
    Ok(vec![YuvFrame {
      pts_us: au.pts_us,
      width: 160,
      height: 120,
      layout,
      data: vec![128; layout.frame_size(160, 120)],
    }])
  }

  fn flush(&mut self) -> Result<Vec<YuvFrame>, String> {
    Ok(Vec::new())
  }
}

#[cfg(not(target_os = "android"))]
#[test]
fn libvpx_decodes_every_frame() {
  let mut demux = WebmDemuxer::open_path(&fixture()).expect("open fixture");
  let mut decoder = crate::video::Vp9Decoder::new(160, 120).expect("create libvpx decoder");
  let mut frames = Vec::new();
  while let Some(au) = demux.next_video().expect("read") {
    frames.extend(decoder.decode(&au).expect("decode"));
  }
  frames.extend(decoder.flush().expect("flush"));
  assert_eq!(frames.len(), 50, "one shown frame per sample");
  for (n, frame) in frames.iter().enumerate() {
    assert_eq!((frame.width, frame.height), (160, 120));
    assert_eq!(frame.layout, PixelLayout::I420);
    assert_eq!(frame.data.len(), PixelLayout::I420.frame_size(160, 120));
    assert_eq!(frame.pts_us, n as i64 * 40_000, "frames carry their sample's pts");
  }
  // testsrc2 is a high-contrast pattern: a decoded luma plane spans most
  // of the range, a mis-stepped copy or a wrong plane offset does not.
  let luma = &frames[0].data[..160 * 120];
  let (lo, hi) = luma.iter().fold((u8::MAX, u8::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
  assert!(hi - lo > 100, "luma range {lo}..{hi} is not a test pattern");
}

#[cfg(not(target_os = "android"))]
#[test]
fn open_plays_the_stream_through_libvpx() {
  // The real factory end to end: the worker builds the libvpx decoder and
  // every frame reaches the consumer against a running clock.
  let mut player = VideoPlayer::open(open_fixture()).expect("open player");
  player.play();
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
  let mut handed = 0;
  for n in 0..50i64 {
    let clock_us = n * 40_000;
    loop {
      while player.next_pcm().is_some() {}
      if let Some(frame) = player.advance(clock_us) {
        assert_eq!(frame.pts_us, clock_us);
        assert_eq!(frame.data.len(), PixelLayout::I420.frame_size(160, 120));
        handed += 1;
        break;
      }
      assert!(std::time::Instant::now() < deadline, "timed out waiting for frame {n}");
      std::thread::sleep(std::time::Duration::from_millis(1));
    }
  }
  assert_eq!(handed, 50);
  assert_eq!(player.position_us(), 49 * 40_000);
}

#[test]
fn player_advances_against_a_caller_clock() {
  let mut player = VideoPlayer::open_with(open_fixture(), |_| Ok(Box::new(StubDecoder))).expect("open player");
  assert_eq!((player.info().width, player.info().height), (160, 120));
  assert_eq!(player.layout(), crate::video::decoded_layout());

  // Paused: nothing comes out no matter the clock.
  assert!(player.advance(1_000_000).is_none());
  player.play();

  // Raise the clock one frame pts at a time and wait for that exact frame:
  // only one frame is ever due, so none can be skipped, whatever the
  // scheduling. The real consumer drains audio to its sink every tick; not
  // draining would backpressure the worker and stall video too (bounded
  // queues).
  let mut pcm = 0usize;
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
  for n in 0..50i64 {
    let clock_us = n * 40_000;
    let frame = loop {
      while let Some(chunk) = player.next_pcm() {
        pcm += chunk.samples.len();
      }
      if let Some(frame) = player.advance(clock_us) {
        break frame;
      }
      assert!(std::time::Instant::now() < deadline, "timed out waiting for frame {n}");
      std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(frame.pts_us, clock_us, "exactly the due frame comes out");
  }
  assert_eq!(player.position_us(), 49 * 40_000);

  // Past the end: the stream closes and the remaining audio drains.
  while !player.finished() && std::time::Instant::now() < deadline {
    while let Some(chunk) = player.next_pcm() {
      pcm += chunk.samples.len();
    }
    assert!(player.advance(2_000_000).is_none(), "nothing after the last frame");
    std::thread::sleep(std::time::Duration::from_millis(1));
  }
  while let Some(chunk) = player.next_pcm() {
    pcm += chunk.samples.len();
  }
  assert!(player.finished());
  let pre_skip = player.info().audio.as_ref().expect("audio").pre_skip as usize;
  assert_eq!(pcm, FIXTURE_AUDIO_PACKETS * OPUS_PACKET_SAMPLES - pre_skip, "all audio reached the consumer");
}

#[test]
fn player_skips_stale_frames_when_the_clock_runs_ahead() {
  let mut player = VideoPlayer::open_with(open_fixture(), |_| Ok(Box::new(StubDecoder))).expect("open player");
  player.play();
  // A clock permanently ahead of the whole clip: each advance drains the
  // queue and hands out only the newest frame, dropping the ones between.
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
  let mut handed = Vec::new();
  while !player.finished() && std::time::Instant::now() < deadline {
    // Drain audio so the worker never blocks on the pcm queue.
    while player.next_pcm().is_some() {}
    if let Some(frame) = player.advance(10_000_000) {
      handed.push(frame.pts_us);
    }
    // Long enough for the worker (microseconds per 160x120 frame) to refill
    // the whole 4-deep queue, so every drain provably has frames to skip.
    std::thread::sleep(std::time::Duration::from_millis(5));
  }
  assert_eq!(*handed.last().expect("some frames"), 49 * 40_000, "the final frame is reached");
  assert!(handed.windows(2).all(|w| w[0] < w[1]), "monotonic order");
  // Each full-queue drain hands 1 of ~5 queued frames, so ~10-13 of 50 in
  // practice; anywhere under 50 proves stale frames drop instead of replay,
  // 25 leaves slack for scheduling noise.
  assert!(handed.len() < 25, "most frames skipped, handed {}", handed.len());
}

#[test]
fn a_stalled_master_clock_plays_out_the_tail() {
  // An audio-clocked stream's clock stops at the end of the audio track,
  // which routinely falls a frame or more short of the last video frame. The tail must still come out, and the stream must end.
  let mut player = VideoPlayer::open_with(open_fixture(), |_| Ok(Box::new(StubDecoder))).expect("open player");
  player.play();
  let stall_us = 44 * 40_000;
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
  let mut last = -1;
  while !player.finished() {
    while player.next_pcm().is_some() {}
    if let Some(frame) = player.advance(stall_us) {
      last = frame.pts_us;
    }
    assert!(std::time::Instant::now() < deadline, "stalled clock never finished, reached {last}us");
    std::thread::sleep(std::time::Duration::from_millis(1));
  }
  assert_eq!(last, 49 * 40_000, "the real final frame is the one left on screen");
  assert_eq!(player.position_us(), 49 * 40_000);
}

#[test]
fn non_webm_input_errs() {
  let err = match WebmDemuxer::open_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")) {
    Ok(_) => panic!("not a webm, open must err"),
    Err(e) => e,
  };
  assert!(err.message.contains("not a webm"), "unexpected error: {err}");
  assert_eq!(err.kind, crate::video::ErrorKind::Unsupported);
}

// 4 s of ffmpeg testsrc2 160x120 at 25 fps as VP9 with a keyframe every
// second (-g 25), no audio: the seek fixture.
fn keyframe_fixture() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_kf.webm").to_string()
}

#[test]
fn seek_lands_on_the_keyframe_at_or_before_the_target() {
  let mut demux = WebmDemuxer::open_path(&keyframe_fixture()).expect("open fixture");
  assert!(demux.info().audio.is_none());
  assert_eq!(demux.info().duration_us, Some(4_000_000));
  assert!(demux.info().seekable);
  assert_eq!(demux.info().start_us, 0);

  assert_eq!(demux.seek(2_500_000).expect("seek"), 2_500_000, "playback resumes at the target");
  let au = demux.next_video().expect("read").expect("a frame");
  assert!(au.sync, "the first frame after a seek is a keyframe");
  assert_eq!(au.pts_us, 2_000_000);
  let next = demux.next_video().expect("read").expect("a frame");
  assert_eq!(next.pts_us, 2_040_000, "and decoding continues from it");

  // Exactly on a keyframe, before the first one, and past the end.
  demux.seek(1_000_000).expect("seek");
  assert_eq!(demux.next_video().expect("read").expect("a frame").pts_us, 1_000_000);
  demux.seek(-5).expect("seek");
  assert_eq!(demux.next_video().expect("read").expect("a frame").pts_us, 0);
  demux.seek(60_000_000).expect("seek");
  assert_eq!(demux.next_video().expect("read").expect("a frame").pts_us, 3_000_000);
}

#[test]
fn seek_repositions_audio_to_the_preroll_before_the_target() {
  let mut demux = WebmDemuxer::open_path(&fixture()).expect("open fixture");
  // The A/V fixture has a single keyframe: video restarts at 0, audio at
  // the first packet at or after the target minus the 80 ms preroll
  // (packets are 20 ms).
  demux.seek(1_000_000).expect("seek");
  let au = demux.next_video().expect("read").expect("a frame");
  assert!(au.sync);
  assert_eq!(au.pts_us, 0);
  let packet = demux.next_audio().expect("read").expect("a packet");
  assert!(packet.pts_us >= 920_000 && packet.pts_us < 940_000, "audio at {}us", packet.pts_us);
  // Back to the start reads the whole audio track again.
  demux.seek(0).expect("seek");
  assert_eq!(demux.next_audio().expect("read").expect("a packet").pts_us, 0);
}

#[test]
fn transport_audio_sync_moves_the_anchor_once_past_the_threshold() {
  use crate::video::transport::{Anchor, AudioSync, AUDIO_SYNC_SMOOTHING, AUDIO_SYNC_THRESHOLD_US};
  let mut sync = AudioSync::new();
  // A device-buffer sawtooth under the threshold never fires.
  for k in 0..100 {
    assert_eq!(sync.observe((k % 5) * 5_000), None, "sawtooth at {k}");
  }
  // A steady lead past the threshold fires once, after the smoothing has
  // caught up, and the smoothing restarts from nothing.
  let lead = AUDIO_SYNC_THRESHOLD_US * 2;
  let mut fired = None;
  for _ in 0..AUDIO_SYNC_SMOOTHING * 4 {
    if let Some(shift) = sync.observe(lead) {
      fired = Some(shift);
      break;
    }
  }
  let shift = fired.expect("a lead past the threshold fires");
  assert!(shift > AUDIO_SYNC_THRESHOLD_US && shift <= lead, "shift {shift}");
  assert_eq!(sync.observe(0), None, "restarted from the corrected state");
  // A lead a stall would hide behind is closed in capped steps, one per
  // frame, never in one move.
  use crate::video::transport::{AUDIO_SYNC_MAX_SHIFT_US, STALL_REANCHOR_NS};
  let mut sync = AudioSync::new();
  assert_eq!(sync.observe(STALL_REANCHOR_NS / 1000 * 3), Some(AUDIO_SYNC_MAX_SHIFT_US));
  assert_eq!(sync.observe(-STALL_REANCHOR_NS / 1000 * 3), Some(-AUDIO_SYNC_MAX_SHIFT_US));
  assert!(AUDIO_SYNC_MAX_SHIFT_US * 1000 < STALL_REANCHOR_NS);

  // The shift moves what the anchor reads at any instant.
  let mut anchor = Anchor::new();
  assert_eq!(anchor.content_at(0), None);
  anchor.due_ns(5_000_000, 1_000_000_000);
  assert_eq!(anchor.content_at(1_500_000_000), Some(5_500_000));
  anchor.shift(40_000);
  assert_eq!(anchor.content_at(1_500_000_000), Some(5_540_000));
  // And a frame at a given pts is now due earlier by the same amount.
  assert_eq!(anchor.due_ns(6_000_000, 0), 1_000_000_000 + 960_000_000);
}

#[test]
fn transport_anchor_starts_on_the_first_frame_and_resets() {
  use crate::video::transport::Anchor;
  let mut anchor = Anchor::new();
  assert!(!anchor.anchored());
  // A stream starting at 7 s anchors there, at the wall time it arrives.
  assert_eq!(anchor.due_ns(7_000_000, 1_000_000_000), 1_000_000_000);
  assert!(anchor.anchored());
  assert_eq!(anchor.due_ns(7_040_000, 1_000_000_000), 1_040_000_000);
  // Wall time passing does not move the anchor.
  assert_eq!(anchor.due_ns(7_080_000, 5_000_000_000), 1_080_000_000);
  // Reset (play, resume, seek, stall): the next frame is due at once.
  anchor.reset();
  assert_eq!(anchor.due_ns(2_000_000, 9_000_000_000), 9_000_000_000);
}

#[test]
fn transport_release_policy_thresholds() {
  use crate::video::transport::{classify, Release, DROP_LATE_NS, RELEASE_LEAD_NS, STALL_REANCHOR_NS};
  let lead = RELEASE_LEAD_NS;
  assert_eq!(classify(lead + 10, lead), Release::Wait(10));
  assert_eq!(classify(lead, lead), Release::AtTime);
  assert_eq!(classify(0, lead), Release::AtTime);
  assert_eq!(classify(-DROP_LATE_NS, lead), Release::AtTime);
  assert_eq!(classify(-DROP_LATE_NS - 1, lead), Release::Drop);
  assert_eq!(classify(-STALL_REANCHOR_NS, lead), Release::Drop);
  assert_eq!(classify(-STALL_REANCHOR_NS - 1, lead), Release::Reanchor);
  // On a known grid the lead is one refresh period.
  assert_eq!(classify(20_000_000, 16_666_667), Release::Wait(3_333_333));
}

#[test]
fn transport_snaps_release_times_onto_the_vsync_grid() {
  use crate::video::transport::{snap_to_vsync, VsyncGrid, VSYNC_OFFSET_PERCENT};
  // A 60 Hz grid sampled at 1 s; the offset pulls each release most of a
  // period before its vsync.
  let period = 16_666_667;
  let offset = period * VSYNC_OFFSET_PERCENT / 100;
  let vsync = 1_000_000_000;
  assert_eq!(snap_to_vsync(vsync, vsync, period), vsync - offset);
  // Just past a vsync goes back to it, just before the next goes forward.
  assert_eq!(snap_to_vsync(vsync + 1_000_000, vsync, period), vsync - offset);
  assert_eq!(snap_to_vsync(vsync + period - 1_000_000, vsync, period), vsync + period - offset);
  // A due time before the sample snaps on the grid extended backwards.
  assert_eq!(snap_to_vsync(vsync - 2 * period + 500_000, vsync, period), vsync - 2 * period - offset);
  // 25 fps on 60 Hz: due times 2.4 periods apart land on two- and
  // three-period steps only (five frames per twelve vsyncs), never a single
  // or a quadruple.
  let mut last = snap_to_vsync(vsync, vsync, period);
  let mut steps = Vec::new();
  for k in 1..=10 {
    let next = snap_to_vsync(vsync + k * 40_000_000, vsync, period);
    steps.push(((next - last) as f64 / period as f64).round() as i64);
    last = next;
  }
  assert_eq!(steps, [2, 3, 2, 3, 2, 2, 3, 2, 3, 2]);
  // Without a sample the grid passes due times through.
  let grid = VsyncGrid { period_ns: period, sample_ns: Box::new(|| None) };
  assert_eq!(grid.snap(123), 123);
  assert_eq!(grid.lead_ns(), period);
  let grid = VsyncGrid { period_ns: period, sample_ns: Box::new(move || Some(vsync)) };
  assert_eq!(grid.snap(vsync + 1_000_000), vsync - offset);
}

#[test]
fn transport_clock_is_monotonic() {
  use crate::video::transport::monotonic_ns;
  let a = monotonic_ns();
  std::thread::sleep(std::time::Duration::from_millis(2));
  let b = monotonic_ns();
  assert!(b >= a + 2_000_000, "{a} -> {b}");
}

#[test]
fn transport_buffering_holds_until_the_resume_buffer_and_resumes_on_the_audio_clock() {
  use crate::video::transport::{
    Anchor, AudioSupply, Buffering, Supply, SINK_LOW_WATER_US, STREAM_LOW_WATER_US, STREAM_RESUME_BUFFER_US,
  };
  let mut buffering = Buffering::new();
  let plenty = Supply { lead_us: STREAM_RESUME_BUFFER_US * 2, ..Supply::default() };
  // Paused: never.
  assert_eq!(buffering.update(false, &Supply::default()), None);
  // Playing with the lead under the low water: starts; the same again is
  // no change; back above the low water but short of the resume buffer:
  // still held; at the resume buffer: ends.
  let short = Supply { lead_us: STREAM_LOW_WATER_US - 1, ..Supply::default() };
  assert_eq!(buffering.update(true, &short), Some(true));
  assert!(buffering.active());
  assert_eq!(buffering.update(true, &short), None);
  let between = Supply { lead_us: STREAM_LOW_WATER_US + 1, ..Supply::default() };
  assert_eq!(buffering.update(true, &between), None);
  assert_eq!(buffering.update(true, &plenty), Some(false));
  assert!(!buffering.active());
  assert_eq!(buffering.update(true, &plenty), None);
  // A pause while buffering ends it; play with a short lead starts again.
  assert_eq!(buffering.update(true, &short), Some(true));
  assert_eq!(buffering.update(false, &short), Some(false));
  assert_eq!(buffering.update(true, &short), Some(true));
  // Nothing more is coming: the end, a failure, the byte cap all end it,
  // and none of them starts it.
  for settled in [Supply { ended: true, ..short }, Supply { failed: true, ..short }, Supply { capped: true, ..short }] {
    assert_eq!(buffering.update(true, &settled), Some(false));
    assert_eq!(buffering.update(true, &settled), None);
    assert_eq!(buffering.update(true, &short), Some(true));
  }
  assert_eq!(buffering.update(true, &plenty), Some(false));
  // The audio side: an empty audio queue with the sink about to underrun
  // starts it, but only short of the resume buffer, so it cannot flap
  // against the exit rule when an audio track ends before the video.
  let starving = Some(AudioSupply { queued_packets: 0, sink_us: SINK_LOW_WATER_US - 1 });
  assert_eq!(buffering.update(true, &Supply { audio: starving, ..plenty }), None);
  let audio_short = Supply { lead_us: STREAM_RESUME_BUFFER_US - 1, audio: starving, ..Supply::default() };
  assert_eq!(buffering.update(true, &audio_short), Some(true));
  assert_eq!(buffering.update(true, &plenty), Some(false));
  let fed = Some(AudioSupply { queued_packets: 3, sink_us: 0 });
  assert_eq!(buffering.update(true, &Supply { audio: fed, ..audio_short }), None);

  // The resume anchors on the audio clock: a frame is due where the sound
  // puts it, not at the wall time it arrives.
  let mut anchor = Anchor::new();
  anchor.set(5_000_000_000, 2_000_000);
  assert!(anchor.anchored());
  assert_eq!(anchor.due_ns(2_040_000, 5_000_000_000), 5_040_000_000);
  assert_eq!(anchor.due_ns(1_960_000, 9_000_000_000), 4_960_000_000);
  assert_eq!(anchor.content_at(5_500_000_000), Some(2_500_000));
}

#[test]
fn transport_constants_are_ordered() {
  use crate::video::audio::AUDIO_LOOKAHEAD_US;
  use crate::video::reader::{PEAK_BITRATE_BPS, STREAM_MAX_BUFFER_BYTES, STREAM_READ_AHEAD_US};
  use crate::video::transport::{RELEASE_LEAD_NS, SINK_LOW_WATER_US, STREAM_LOW_WATER_US, STREAM_RESUME_BUFFER_US};
  // The reader reads ahead past the resume buffer plus the sink's share.
  assert!(STREAM_READ_AHEAD_US > STREAM_RESUME_BUFFER_US + AUDIO_LOOKAHEAD_US);
  // The byte cap holds the read-ahead at the peak bitrate.
  let peak_bytes = PEAK_BITRATE_BPS / 8 * (STREAM_READ_AHEAD_US as usize / 1_000_000);
  assert!(STREAM_MAX_BUFFER_BYTES > peak_bytes, "{STREAM_MAX_BUFFER_BYTES} <= {peak_bytes}");
  // Buffering starts before the picture or the sound can starve.
  assert!(STREAM_LOW_WATER_US > SINK_LOW_WATER_US + RELEASE_LEAD_NS / 1000);
  assert!(STREAM_RESUME_BUFFER_US > STREAM_LOW_WATER_US);
}
