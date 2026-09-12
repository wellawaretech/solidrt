// The player: a decode worker thread feeding bounded queues, and a
// clock-agnostic consumer surface. The caller owns the master clock (the
// audio sink position when there is audio, wall time otherwise) and calls
// `advance(clock_us)` per tick; the player returns the frame due at that
// clock, skipping stale ones, and hands decoded PCM out for the caller's
// sink. Sync DECISIONS live here; clock and upload mechanics live with the
// caller (see okf/backlog/video-playback.md).

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::thread;

use super::aac::{AacDecoder, PcmChunk};
use super::demux::{MediaInfo, Mp4Demuxer};
use super::{PixelLayout, VideoDecoder, YuvFrame};

// Frames are ~3 MB at 1080p, so the lookahead is small; PCM chunks are ~4 KB
// AAC frames, so a deeper queue (~1.5 s at 44.1 kHz) keeps audio fed while
// the consumer paces itself against its sink.
const FRAME_QUEUE: usize = 4;
const PCM_QUEUE: usize = 64;
// Frames held on the consumer side, ahead of the one being shown. Two is
// the minimum that lets `advance` learn end of stream: with a frame in
// hand it still has room to attempt a receive, and the attempt is what
// reports the worker's exit. Deeper buys nothing - the channel above is
// the real lookahead - and costs ~3 MB per frame at 1080p.
const STAGE_QUEUE: usize = 2;
// Calls with an unmoved clock after which the caller's master clock counts
// as stopped rather than merely coarse. An audio sink position advances in
// callback chunks, so repeating for a call or two is normal; at 50 calls a
// second this is a fifth of a second of a clock that is pacing nothing,
// which only happens once its track has run out.
const STALLED_CALLS: u32 = 10;

pub struct VideoPlayer {
  info: MediaInfo,
  layout: PixelLayout,
  bt709: bool,
  playing: bool,
  frame_rx: Receiver<YuvFrame>,
  pcm_rx: Receiver<PcmChunk>,
  staged: VecDeque<YuvFrame>,
  video_done: bool,
  position_us: i64,
  last_clock_us: i64,
  stalled_calls: u32,
}

impl VideoPlayer {
  /// Open a local MP4 and start its decode worker. The worker prefetches
  /// until the queues fill, so opening is cheap and nothing plays until the
  /// caller starts advancing the clock. Errs on an unreadable file, an
  /// unsupported video codec, or a platform with no decoder.
  ///
  /// Opening never blocks on the decoder itself. Constructing one is the
  /// platform's business and can take as long as it likes (a hardware codec
  /// may be waiting for an instance the previous player has yet to release),
  /// and this runs on the caller's thread - the same thread that has to
  /// close that previous player. So the worker builds the decoder, and a
  /// failure there ends the stream like any other: the queues close,
  /// `finished` goes true, and the reason is logged.
  pub fn open(path: &str) -> Result<VideoPlayer, String> {
    if !platform_has_decoder() {
      return Err(NO_DECODER.to_string());
    }
    Self::open_with(path, create_decoder)
  }

  /// The seam `open` is built on: the decoder factory runs ON the worker
  /// thread, because platform decoder handles are not `Send`. Tests pass a
  /// stub here, which is the only way to exercise playback on a host that
  /// has no decoder of its own.
  pub(crate) fn open_with(path: &str, make_decoder: DecoderFactory) -> Result<VideoPlayer, String> {
    let mut demux = Mp4Demuxer::open(path)?;
    let info = demux.info().clone();
    let layout = super::decoded_layout();
    let bt709 = demux.color_is_bt709();
    let audio = info.audio.clone();

    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel(FRAME_QUEUE);
    let (pcm_tx, pcm_rx) = std::sync::mpsc::sync_channel(PCM_QUEUE);
    thread::Builder::new()
      .name("srt-video".to_string())
      .spawn(move || worker(&mut demux, make_decoder, audio.as_ref(), &frame_tx, &pcm_tx))
      .map_err(|e| format!("spawn video worker: {e}"))?;

    Ok(VideoPlayer {
      info,
      layout,
      bt709,
      playing: false,
      frame_rx,
      pcm_rx,
      staged: VecDeque::with_capacity(STAGE_QUEUE),
      video_done: false,
      position_us: 0,
      last_clock_us: i64::MIN,
      stalled_calls: 0,
    })
  }

  pub fn info(&self) -> &MediaInfo {
    &self.info
  }

  /// The layout decoded frames arrive in (fixed per decoder).
  pub fn layout(&self) -> PixelLayout {
    self.layout
  }

  /// The color default for this stream: BT.709 for HD, BT.601 for SD.
  pub fn color_is_bt709(&self) -> bool {
    self.bt709
  }

  pub fn play(&mut self) {
    self.playing = true;
  }

  pub fn pause(&mut self) {
    self.playing = false;
  }

  pub fn playing(&self) -> bool {
    self.playing
  }

  /// Presentation time of the last frame handed out.
  pub fn position_us(&self) -> i64 {
    self.position_us
  }

  /// The frame due at `clock_us`, if a new one is: the latest staged frame
  /// with pts <= clock, older ones dropped (frame skipping under a clock
  /// that ran ahead). None while paused, when the due frame is unchanged,
  /// or when decode has not caught up (the caller keeps showing the last
  /// frame; a texture upload only happens on Some).
  ///
  /// The tail is the one case the clock alone cannot resolve. A master
  /// clock reads the audio sink, and an MP4's audio track routinely ends
  /// before its last video frame, so those frames are due at a time the
  /// clock will never reach. Once the clock has stopped moving for long
  /// enough to mean its track ran out (see STALLED_CALLS), the rest is
  /// released one frame per call - at the caller's tick rate, which for
  /// the frame or two this normally amounts to is exactly right - so a
  /// stream ends on its real final frame instead of hanging short of it
  /// forever.
  pub fn advance(&mut self, clock_us: i64) -> Option<YuvFrame> {
    if !self.playing {
      return None;
    }
    if clock_us > self.last_clock_us {
      self.stalled_calls = 0;
    } else {
      self.stalled_calls = self.stalled_calls.saturating_add(1);
    }
    self.last_clock_us = clock_us;
    let stalled = self.stalled_calls >= STALLED_CALLS;

    let mut due: Option<YuvFrame> = None;
    loop {
      // Top the staging queue up; the attempt is also what reports the
      // worker's exit, so end of stream is known while a frame is still in
      // hand. Refilling inside the loop keeps a clock that ran ahead able
      // to drain the whole channel in one call.
      while self.staged.len() < STAGE_QUEUE {
        match self.frame_rx.try_recv() {
          Ok(frame) => self.staged.push_back(frame),
          Err(TryRecvError::Empty) => break,
          Err(TryRecvError::Disconnected) => {
            self.video_done = true;
            break;
          }
        }
      }
      match self.staged.front() {
        Some(frame) if frame.pts_us <= clock_us => due = self.staged.pop_front(),
        Some(_) if stalled && due.is_none() => {
          due = self.staged.pop_front();
          break;
        }
        _ => break,
      }
    }
    if let Some(frame) = &due {
      self.position_us = frame.pts_us;
    }
    due
  }

  /// Drain one decoded PCM chunk for the caller's audio sink, None when the
  /// worker has nothing ready. The caller paces itself against its sink's
  /// queue; the bounded channel backpressures the worker.
  pub fn next_pcm(&mut self) -> Option<PcmChunk> {
    self.pcm_rx.try_recv().ok()
  }

  /// Whether every frame has been decoded AND handed out.
  pub fn finished(&self) -> bool {
    self.video_done && self.staged.is_empty()
  }
}

// Decodes both streams in pts order into the bounded queues. Blocking sends
// are the backpressure; when the player drops, the sends fail and the worker
// exits. Per-AU decode errors skip the AU (fail-soft playback: one bad AU
// must not kill the stream).
// The platform decoder: MediaCodec on Android, and nothing anywhere else
// until the remaining platform rungs land (VA-API, V4L2, VideoToolbox,
// Media Foundation - see the note). There is no software fallback by
// design; a creation failure ends the stream like any decoder-init failure.
/// How a worker gets its decoder. A plain fn pointer: the factory crosses
/// to the worker thread, the decoder itself never does.
pub(crate) type DecoderFactory = fn(&Mp4Demuxer) -> Result<Box<dyn VideoDecoder>, String>;

const NO_DECODER: &str = "no video decoder on this platform yet (H.264 decode is implemented for Android only)";

/// Whether this platform has a decoder at all - a compile-time fact, since
/// the rungs land one platform at a time (see the note). Where none has,
/// opening fails outright rather than starting a worker that can only
/// report having nothing to decode with.
fn platform_has_decoder() -> bool {
  cfg!(target_os = "android")
}

// A hardware decoder is a limited resource: this device allows few enough
// AVC instances that switching clips - where the incoming player is built
// before the outgoing one is closed - can find them all taken. Retry across
// that handover rather than failing a clip for a codec that is about to be
// free. Total wait stays well inside the time a player takes to start.
#[cfg(target_os = "android")]
const DECODER_ATTEMPTS: u32 = 10;
#[cfg(target_os = "android")]
const DECODER_RETRY_MS: u64 = 50;

#[cfg(target_os = "android")]
fn create_decoder(demux: &Mp4Demuxer) -> Result<Box<dyn VideoDecoder>, String> {
  let info = demux.info();
  let (sps, pps) = demux.parameter_sets();
  let mut last = String::new();
  for attempt in 0..DECODER_ATTEMPTS {
    match super::mediacodec::MediaCodecDecoder::new(info.width, info.height, sps, pps) {
      Ok(decoder) => return Ok(Box::new(decoder)),
      Err(e) => last = e,
    }
    if attempt + 1 < DECODER_ATTEMPTS {
      thread::sleep(std::time::Duration::from_millis(DECODER_RETRY_MS));
    }
  }
  Err(format!("{last} (after {DECODER_ATTEMPTS} attempts)"))
}

#[cfg(not(target_os = "android"))]
fn create_decoder(_demux: &Mp4Demuxer) -> Result<Box<dyn VideoDecoder>, String> {
  Err(NO_DECODER.to_string())
}

fn worker(
  demux: &mut Mp4Demuxer,
  make_decoder: DecoderFactory,
  audio: Option<&super::demux::AudioInfo>,
  frame_tx: &SyncSender<YuvFrame>,
  pcm_tx: &SyncSender<PcmChunk>,
) {
  let mut decoder = match make_decoder(demux) {
    Ok(d) => d,
    Err(e) => {
      // Dropping the senders here is what the consumer reads as end of
      // stream, so a stream with no decoder ends instead of hanging.
      log::warn!("[forge::video] {e}");
      return;
    }
  };
  let mut aac = match audio.map(AacDecoder::new).transpose() {
    Ok(d) => d,
    Err(e) => {
      log::warn!("[forge::video] {e} (playing silent)");
      None
    }
  };

  let mut next_video = demux.next_video().unwrap_or_else(|e| {
    log::warn!("[forge::video] {e}");
    None
  });
  let mut next_audio = if aac.is_some() { read_audio(demux) } else { None };

  loop {
    // Feed in pts order so neither bounded queue starves the other.
    let video_turn = match (&next_video, &next_audio) {
      (Some(v), Some(a)) => v.pts_us <= a.pts_us,
      (Some(_), None) => true,
      (None, Some(_)) => false,
      (None, None) => break,
    };
    if video_turn {
      let au = next_video.take().expect("video_turn implies an AU");
      match decoder.decode(&au) {
        Ok(frames) => {
          for frame in frames {
            if frame_tx.send(frame).is_err() {
              return;
            }
          }
        }
        Err(e) => log::warn!("[forge::video] skipping AU at {}us: {e}", au.pts_us),
      }
      next_video = demux.next_video().unwrap_or_else(|e| {
        log::warn!("[forge::video] {e}");
        None
      });
    } else {
      let packet = next_audio.take().expect("audio turn implies a packet");
      let dec = aac.as_mut().expect("audio packets only flow with a decoder");
      match dec.decode(packet.pts_us, &packet.data) {
        Ok(chunk) => {
          if pcm_tx.send(chunk).is_err() {
            return;
          }
        }
        Err(e) => log::warn!("[forge::video] skipping audio packet at {}us: {e}", packet.pts_us),
      }
      next_audio = read_audio(demux);
    }
  }

  match decoder.flush() {
    Ok(frames) => {
      for frame in frames {
        if frame_tx.send(frame).is_err() {
          return;
        }
      }
    }
    Err(e) => log::warn!("[forge::video] {e}"),
  }
  // Senders drop here; the receivers read that as end of stream.
}

fn read_audio(demux: &mut Mp4Demuxer) -> Option<super::demux::AudioPacket> {
  demux.next_audio().unwrap_or_else(|e| {
    log::warn!("[forge::video] {e}");
    None
  })
}
