// The demuxer over an untrusted, possibly unseekable source: one byte per
// read, unknown sizes, truncation, garbage sizes, keyframe gating, lazy
// cues, a stream that restarts. The fixtures are read into memory and bent
// here; nothing on disk changes.

use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use crate::seek::SeekableReader;
use crate::source::Facts;
use crate::video::{Demuxer, ErrorKind, StreamError, WebmDemuxer};

const ID_SEGMENT: &[u8] = &[0x18, 0x53, 0x80, 0x67];
const ID_CLUSTER: &[u8] = &[0x1F, 0x43, 0xB6, 0x75];
const ID_CUES: &[u8] = &[0x1C, 0x53, 0xBB, 0x6B];
const ID_SIMPLE_BLOCK: u8 = 0xA3;
const ID_VOID: u8 = 0xEC;
// A size no stream can mean: refused before allocation.
const GARBAGE_SIZE: u64 = 1 << 40;

fn av_fixture() -> Vec<u8> {
  std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_av.webm")).expect("read fixture")
}

// 4 s of testsrc2 160x120 at 25 fps as VP9 (-g 25) with a 440 Hz sine as
// mono Opus, clusters every 500 ms (-cluster_time_limit 500): the odd
// clusters open on a non-keyframe video block, and the Cues sit at the
// tail. The gating and lazy-cues fixture.
fn gop_fixture() -> Vec<u8> {
  std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_gop.webm")).expect("read fixture")
}

fn open_bytes(bytes: Vec<u8>, seekable: bool) -> Result<WebmDemuxer, StreamError> {
  let len = bytes.len() as u64;
  WebmDemuxer::open(Box::new(Cursor::new(bytes)), Facts { len: Some(len), seekable })
}

fn open_error(bytes: Vec<u8>, seekable: bool, why: &str) -> StreamError {
  match open_bytes(bytes, seekable) {
    Ok(_) => panic!("open must fail: {why}"),
    Err(e) => e,
  }
}

/// Every video frame and audio packet until the end or an error.
fn drain(demux: &mut WebmDemuxer) -> (Vec<(i64, bool)>, Vec<i64>, Result<(), StreamError>) {
  let mut video = Vec::new();
  let mut audio = Vec::new();
  loop {
    match demux.next_video() {
      Ok(Some(au)) => video.push((au.pts_us, au.sync)),
      Ok(None) => break,
      Err(e) => return (video, audio, Err(e)),
    }
  }
  loop {
    match demux.next_audio() {
      Ok(Some(packet)) => audio.push(packet.pts_us),
      Ok(None) => break,
      Err(e) => return (video, audio, Err(e)),
    }
  }
  (video, audio, Ok(()))
}

// --- EBML surgery ---

/// (value, encoded length) of the vint at `at`.
fn vint(bytes: &[u8], at: usize) -> (u64, usize) {
  let len = bytes[at].leading_zeros() as usize + 1;
  let mut value = (bytes[at] & (0xFFu16 >> len) as u8) as u64;
  for &b in &bytes[at + 1..at + len] {
    value = (value << 8) | b as u64;
  }
  (value, len)
}

/// An unknown-size marker of `len` bytes: every value bit set.
fn unknown_size(len: usize) -> Vec<u8> {
  let mut out = vec![0xFF; len];
  out[0] = (0xFFu16 >> (len - 1)) as u8;
  out
}

/// An 8-byte vint of `value`.
fn size8(value: u64) -> Vec<u8> {
  let mut out = value.to_be_bytes().to_vec();
  out[0] |= 0x01;
  out
}

fn find(bytes: &[u8], id: &[u8], from: usize) -> Option<usize> {
  bytes[from..].windows(id.len()).position(|w| w == id).map(|p| p + from)
}

/// The elements in `bytes[from..to]`: (id offset, size vint length, size,
/// data offset).
fn children(bytes: &[u8], from: usize, to: usize) -> Vec<(usize, usize, u64, usize)> {
  let mut pos = from;
  let mut out = Vec::new();
  while pos < to {
    let id_len = bytes[pos].leading_zeros() as usize + 1;
    let (size, size_len) = vint(bytes, pos + id_len);
    let data = pos + id_len + size_len;
    out.push((pos, size_len, size, data));
    pos = data + size as usize;
  }
  out
}

/// Top-level children of the Segment.
fn segment_children(bytes: &[u8]) -> Vec<(usize, usize, u64, usize)> {
  let segment = find(bytes, ID_SEGMENT, 0).expect("a segment");
  let (_, size_len) = vint(bytes, segment + 4);
  children(bytes, segment + 4 + size_len, bytes.len())
}

/// The payload span of the last block in the last cluster.
fn last_block_payload(bytes: &[u8]) -> (usize, usize) {
  let (_, _, size, data) =
    segment_children(bytes).into_iter().filter(|&(at, ..)| &bytes[at..at + 4] == ID_CLUSTER).last().expect("a cluster");
  let (at, _, size, data) = children(bytes, data, data + size as usize)
    .into_iter()
    .filter(|&(at, ..)| bytes[at] == ID_SIMPLE_BLOCK)
    .last()
    .expect("a block");
  let _ = at;
  (data, data + size as usize)
}

fn cluster_offsets(bytes: &[u8]) -> Vec<usize> {
  segment_children(bytes).into_iter().filter(|&(at, ..)| &bytes[at..at + 4] == ID_CLUSTER).map(|c| c.0).collect()
}

/// The Segment and every Cluster rewritten to an unknown size, in place.
fn with_unknown_sizes(mut bytes: Vec<u8>) -> Vec<u8> {
  let clusters: Vec<(usize, usize)> = segment_children(&bytes)
    .into_iter()
    .filter(|&(at, ..)| &bytes[at..at + 4] == ID_CLUSTER)
    .map(|(at, size_len, ..)| (at + 4, size_len))
    .collect();
  for (size_at, size_len) in clusters {
    bytes[size_at..size_at + size_len].copy_from_slice(&unknown_size(size_len));
  }
  let segment = find(&bytes, ID_SEGMENT, 0).expect("a segment");
  let (_, size_len) = vint(&bytes, segment + 4);
  bytes[segment + 4..segment + 4 + size_len].copy_from_slice(&unknown_size(size_len));
  bytes
}

/// The stream's header (through the Segment's children before the first
/// Cluster), sized unknown.
fn header(bytes: &[u8]) -> Vec<u8> {
  let first_cluster = cluster_offsets(bytes)[0];
  with_unknown_sizes(bytes[..first_cluster].to_vec())
}

// --- Readers ---

/// Hands out one byte per read.
struct OneByte(Cursor<Vec<u8>>);

impl Read for OneByte {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    let n = buf.len().min(1);
    self.0.read(&mut buf[..n])
  }
}

impl Seek for OneByte {
  fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
    self.0.seek(pos)
  }
}

/// Records the furthest byte the demuxer has touched.
struct Tracing {
  inner: Cursor<Vec<u8>>,
  reach: Arc<Mutex<u64>>,
}

impl Tracing {
  fn note(&self) {
    let mut reach = self.reach.lock().expect("reach");
    *reach = (*reach).max(self.inner.position());
  }
}

impl Read for Tracing {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    let n = self.inner.read(buf)?;
    self.note();
    Ok(n)
  }
}

impl Seek for Tracing {
  fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
    let at = self.inner.seek(pos)?;
    self.note();
    Ok(at)
  }
}

// --- Tests ---

#[test]
fn one_byte_reads_walk_the_whole_stream() {
  let bytes = av_fixture();
  let reader: SeekableReader = Box::new(OneByte(Cursor::new(bytes.clone())));
  let mut demux = WebmDemuxer::open(reader, Facts { len: Some(bytes.len() as u64), seekable: true }).expect("open");
  let (video, audio, end) = drain(&mut demux);
  end.expect("clean end");
  assert_eq!(video.len(), 50);
  assert_eq!(audio.len(), 101);
}

#[test]
fn unknown_sizes_hide_the_duration_and_still_play() {
  let bytes = with_unknown_sizes(av_fixture());
  let mut demux = open_bytes(bytes, false).expect("open");
  let info = demux.info();
  assert_eq!(info.duration_us, None, "a Duration in an unknown-size Segment is not a fact");
  assert!(!info.seekable);
  assert_eq!(info.start_us, 0);

  // A seek on a source that cannot seek errs and changes nothing.
  for _ in 0..10 {
    demux.next_video().expect("read").expect("a frame");
  }
  demux.seek(1_000_000).expect_err("cannot seek");
  assert_eq!(demux.next_video().expect("read").expect("a frame").pts_us, 10 * 40_000);

  let (video, audio, end) = drain(&mut demux);
  end.expect("clean end");
  assert_eq!(video.len(), 39);
  assert_eq!(audio.len(), 101);
}

#[test]
fn truncation_anywhere_in_the_last_cluster_ends_or_errs_without_panic() {
  let bytes = gop_fixture();
  let last_cluster = *cluster_offsets(&bytes).last().expect("clusters");
  let whole = {
    let mut demux = open_bytes(bytes.clone(), true).expect("open");
    drain(&mut demux)
  };
  assert_eq!(whole.0.len(), 100);
  for cut in last_cluster..bytes.len() {
    let mut demux = match open_bytes(bytes[..cut].to_vec(), true) {
      Ok(demux) => demux,
      Err(e) => panic!("the header is intact, open must succeed at cut {cut}: {e}"),
    };
    let (video, audio, end) = drain(&mut demux);
    assert!(video.len() <= whole.0.len() && audio.len() <= whole.1.len(), "cut {cut}");
    assert!(video.iter().zip(&whole.0).all(|(a, b)| a == b), "cut {cut}: frames stay in order");
    if end.is_err() {
      // Sticky: the same error again, no frames after it.
      assert!(demux.next_video().is_err(), "cut {cut}");
    }
  }
  // Cut inside the last block's payload: an error, never a clean end.
  let (payload_start, payload_end) = last_block_payload(&bytes);
  for cut in [payload_start + 1, (payload_start + payload_end) / 2, payload_end - 1] {
    let mut demux = open_bytes(bytes[..cut].to_vec(), true).expect("open");
    let (_, _, end) = drain(&mut demux);
    end.expect_err("a block cut short is an error");
    // Packets read intact before the fault still come out; then the
    // error, on both tracks.
    while let Ok(Some(_)) = demux.next_audio() {}
    assert!(demux.next_video().is_err() && demux.next_audio().is_err(), "cut {cut}: sticky on both tracks");
  }
}

#[test]
fn garbage_sizes_error_before_any_allocation() {
  // A block claiming a terabyte.
  let bytes = av_fixture();
  let first_cluster = cluster_offsets(&bytes)[0];
  let block = find(&bytes, &[ID_SIMPLE_BLOCK], first_cluster).expect("a block");
  let (_, size_len) = vint(&bytes, block + 1);
  let mut bent = bytes[..block + 1].to_vec();
  bent.extend(size8(GARBAGE_SIZE));
  bent.extend_from_slice(&bytes[block + 1 + size_len..]);
  let err = open_error(bent, true, "a garbage block size");
  assert!(err.message.contains("exceeds"), "{err}");
  assert_eq!(err.kind, ErrorKind::Decode);

  // A skip past a terabyte on a source that cannot seek.
  let mut bent = bytes[..first_cluster].to_vec();
  bent.push(ID_VOID);
  bent.extend(size8(GARBAGE_SIZE));
  bent.extend_from_slice(&bytes[first_cluster..]);
  let err = open_error(with_unknown_sizes(bent), false, "a garbage skip");
  assert!(err.message.contains("exceeds"), "{err}");
}

#[test]
fn an_epoch_starting_mid_gop_waits_for_the_keyframe_and_keeps_the_audio_preroll() {
  // The stream from its second cluster on: video from 520 ms, all
  // non-keyframes until the keyframe at 1000 ms (in the cluster whose
  // timecode says 1001).
  let bytes = gop_fixture();
  let clusters = cluster_offsets(&bytes);
  let mut cut = header(&bytes);
  cut.extend_from_slice(&bytes[clusters[1]..]);
  let mut demux = open_bytes(cut, false).expect("open");
  assert_eq!(demux.info().start_us, 1_000_000, "the first keyframe is the start");
  let preroll_us = demux.info().audio.as_ref().expect("audio").seek_preroll_us;
  assert_eq!(preroll_us, 80_000);

  let first = demux.next_video().expect("read").expect("a frame");
  assert!(first.sync);
  assert_eq!(first.pts_us, 1_000_000);
  let packet = demux.next_audio().expect("read").expect("a packet");
  assert!(packet.pts_us >= 920_000 && packet.pts_us < 940_000, "audio from the preroll, at {}us", packet.pts_us);
  assert_eq!(demux.next_video().expect("read").expect("a frame").pts_us, 1_040_000);
}

#[test]
fn a_seek_resumes_at_the_target_from_the_keyframe_before_it() {
  let mut demux = open_bytes(gop_fixture(), true).expect("open");
  assert_eq!(demux.info().start_us, 0);
  assert_eq!(demux.seek(1_500_000).expect("seek"), 1_500_000);
  let first = demux.next_video().expect("read").expect("a frame");
  assert!(first.sync);
  assert_eq!(first.pts_us, 1_000_000, "decoding starts at the keyframe before the target");
  let packet = demux.next_audio().expect("read").expect("a packet");
  assert!(packet.pts_us >= 1_420_000 && packet.pts_us < 1_440_000, "audio at {}us", packet.pts_us);
}

#[test]
fn open_never_visits_tail_cues_and_the_first_seek_loads_them() {
  let bytes = gop_fixture();
  let cues = find(&bytes, ID_CUES, cluster_offsets(&bytes)[0]).expect("tail cues") as u64;
  let first_cluster_end = segment_children(&bytes)
    .into_iter()
    .find(|&(at, ..)| &bytes[at..at + 4] == ID_CLUSTER)
    .map(|(_, _, size, data)| data + size as usize)
    .expect("a cluster") as u64;
  let reach = Arc::new(Mutex::new(0u64));
  let reader: SeekableReader = Box::new(Tracing { inner: Cursor::new(bytes.clone()), reach: reach.clone() });
  let mut demux = WebmDemuxer::open(reader, Facts { len: Some(bytes.len() as u64), seekable: true }).expect("open");
  let reached = *reach.lock().expect("reach");
  assert!(reached <= first_cluster_end, "open read to {reached}, past the first cluster");

  assert_eq!(demux.seek(2_000_000).expect("seek"), 2_000_000);
  assert!(*reach.lock().expect("reach") > cues, "the seek visited the cues");
  let au = demux.next_video().expect("read").expect("a frame");
  assert!(au.sync);
  assert_eq!(au.pts_us, 2_000_000);
}

#[test]
fn a_new_ebml_header_ends_the_stream_as_unsupported() {
  let bytes = gop_fixture();
  let clusters = cluster_offsets(&bytes);
  let mut joined = header(&bytes);
  joined.extend_from_slice(&bytes[clusters[0]..clusters[1]]);
  joined.extend_from_slice(&bytes);
  let mut demux = open_bytes(joined, false).expect("open");
  let (video, _, end) = drain(&mut demux);
  let err = end.expect_err("a second stream is not followed");
  assert_eq!(err.kind, ErrorKind::Unsupported, "{err}");
  assert_eq!(video.len(), 13, "the first cluster's frames (0 to 480 ms) came out");
  assert!(demux.next_video().is_err(), "sticky");
}

#[test]
fn an_unplayed_track_is_dropped_at_the_source() {
  let mut demux = open_bytes(av_fixture(), true).expect("open");
  demux.set_tracks(true, false);
  assert!(demux.next_audio().expect("read").is_none());
  let (video, _, end) = drain(&mut demux);
  end.expect("clean end");
  assert_eq!(video.len(), 50);
}
