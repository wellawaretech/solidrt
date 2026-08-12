use crate::video::{AacDecoder, H264Decoder, Mp4Demuxer, PixelLayout, VideoDecoder};

// 2 s of ffmpeg testsrc2 160x120 at 25 fps (H.264 high profile, no
// B-frames - the openh264 decoder has no B-slice support) with a 440 Hz
// sine as mono AAC at 44100 Hz.
fn fixture() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_av.mp4").to_string()
}

#[test]
fn header_reports_the_stream_facts() {
  let demux = Mp4Demuxer::open(&fixture()).expect("open fixture");
  let info = demux.info();
  assert_eq!((info.width, info.height), (160, 120));
  assert_eq!(info.frame_count, 50);
  assert!((info.duration_us - 2_000_000).abs() < 100_000, "duration {} not ~2s", info.duration_us);
  let audio = info.audio.as_ref().expect("audio track");
  assert_eq!((audio.sample_rate, audio.channels), (44100, 1));
  // AAC-LC (object type 2), 44100 (freq index 4), mono (channel config 1).
  assert_eq!(audio.asc, vec![0x12, 0x08]);
  assert!(!demux.color_is_bt709(), "SD content defaults to BT.601");
}

#[test]
fn video_aus_are_annexb_with_sps_pps_at_sync() {
  let mut demux = Mp4Demuxer::open(&fixture()).expect("open fixture");
  let first = demux.next_video().expect("read").expect("first AU");
  assert!(first.sync, "first AU is the keyframe");
  assert_eq!(first.pts_us, 0);
  assert_eq!(&first.data[..4], &[0, 0, 0, 1], "Annex-B start code");
  assert_eq!(first.data[4] & 0x1f, 7, "sync AU starts with the SPS");

  let mut count = 1;
  let mut last_pts = first.pts_us;
  while let Some(au) = demux.next_video().expect("read") {
    assert!(au.pts_us > last_pts, "pts must be monotonic without B-frames");
    assert_eq!(au.pts_us - last_pts, 40_000, "25 fps spacing");
    assert_eq!(&au.data[..4], &[0, 0, 0, 1]);
    last_pts = au.pts_us;
    count += 1;
  }
  assert_eq!(count, 50);
  assert!(demux.next_video().expect("read past end").is_none(), "stays exhausted");
}

#[test]
fn openh264_decodes_the_full_clip_to_packed_i420() {
  let mut demux = Mp4Demuxer::open(&fixture()).expect("open fixture");
  let mut decoder = H264Decoder::new().expect("create decoder");
  let mut frames = Vec::new();
  while let Some(au) = demux.next_video().expect("read") {
    frames.extend(decoder.decode(&au).expect("decode AU"));
  }
  frames.extend(decoder.flush().expect("flush"));
  assert_eq!(frames.len(), 50);

  let expected_len = PixelLayout::I420.frame_size(160, 120);
  for (i, frame) in frames.iter().enumerate() {
    assert_eq!((frame.width, frame.height), (160, 120));
    assert_eq!(frame.layout, PixelLayout::I420);
    assert_eq!(frame.data.len(), expected_len);
    assert_eq!(frame.pts_us, i as i64 * 40_000, "output pts pairs back to its AU");
  }
  // Content sanity: testsrc2 is colorful, so the Y plane is not flat and
  // the chroma planes are not neutral.
  let y = &frames[10].data[..160 * 120];
  let (min, max) = y.iter().fold((255u8, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));
  assert!(max - min > 100, "Y plane should span a wide range, got {min}..{max}");
  let u = &frames[10].data[160 * 120..160 * 120 + 80 * 60];
  assert!(u.iter().any(|&v| (v as i32 - 128).abs() > 30), "U plane should carry color");
}

#[test]
fn aac_decodes_every_packet_to_pcm() {
  let mut demux = Mp4Demuxer::open(&fixture()).expect("open fixture");
  let info = demux.info().audio.as_ref().expect("audio track");
  let mut decoder = AacDecoder::new(info).expect("create aac decoder");
  let mut packets = 0;
  let mut pcm_frames = 0;
  let mut peak = 0.0f32;
  while let Some(packet) = demux.next_audio().expect("read") {
    let chunk = decoder.decode(packet.pts_us, &packet.data).expect("decode");
    assert_eq!((chunk.sample_rate, chunk.channels), (44100, 1));
    pcm_frames += chunk.samples.len() / chunk.channels as usize;
    peak = chunk.samples.iter().fold(peak, |p, &s| p.max(s.abs()));
    packets += 1;
  }
  assert_eq!(packets, 88);
  assert_eq!(pcm_frames, 88 * 1024, "AAC frames are 1024 samples");
  // A sine at default ffmpeg volume: clearly audible, never clipping.
  assert!(peak > 0.1 && peak <= 1.0, "peak {peak} out of range");
}

#[test]
fn player_advances_against_a_caller_clock() {
  let mut player = crate::video::VideoPlayer::open(&fixture()).expect("open player");
  assert_eq!((player.info().width, player.info().height), (160, 120));
  assert_eq!(player.layout(), PixelLayout::I420);

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
  assert_eq!(pcm, 88 * 1024, "all audio reached the consumer");
}

#[test]
fn player_skips_stale_frames_when_the_clock_runs_ahead() {
  let mut player = crate::video::VideoPlayer::open(&fixture()).expect("open player");
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
fn non_mp4_input_errs() {
  let err = match Mp4Demuxer::open(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")) {
    Ok(_) => panic!("not an mp4, open must err"),
    Err(e) => e,
  };
  assert!(err.contains("header"), "unexpected error: {err}");
}
