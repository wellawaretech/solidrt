// The player: a decode worker thread feeding bounded queues, and a
// clock-agnostic consumer surface. The caller owns the master clock (the
// audio sink position when there is audio, wall time otherwise) and calls
// `advance(clock_us)` per tick; the player returns the frame due at that
// clock, skipping stale ones, and hands decoded PCM out for the caller's
// sink. Sync DECISIONS live here; clock and upload mechanics live with the
// caller (see okf/backlog/video-playback.md).

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

pub struct VideoPlayer {
  info: MediaInfo,
  layout: PixelLayout,
  bt709: bool,
  playing: bool,
  frame_rx: Receiver<YuvFrame>,
  pcm_rx: Receiver<PcmChunk>,
  staged: Option<YuvFrame>,
  video_done: bool,
  position_us: i64,
}

impl VideoPlayer {
  /// Open a local MP4 and start its decode worker. The worker prefetches
  /// until the queues fill, so opening is cheap and nothing plays until the
  /// caller starts advancing the clock. Errs on an unreadable file or an
  /// unsupported video codec.
  pub fn open(path: &str) -> Result<VideoPlayer, String> {
    let mut demux = Mp4Demuxer::open(path)?;
    let info = demux.info().clone();
    let layout = super::decoded_layout();
    let bt709 = demux.color_is_bt709();
    let audio = info.audio.clone();

    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel(FRAME_QUEUE);
    let (pcm_tx, pcm_rx) = std::sync::mpsc::sync_channel(PCM_QUEUE);
    thread::Builder::new()
      .name("srt-video".to_string())
      .spawn(move || worker(&mut demux, audio.as_ref(), &frame_tx, &pcm_tx))
      .map_err(|e| format!("spawn video worker: {e}"))?;

    Ok(VideoPlayer {
      info,
      layout,
      bt709,
      playing: false,
      frame_rx,
      pcm_rx,
      staged: None,
      video_done: false,
      position_us: 0,
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

  /// The frame due at `clock_us`, if a new one is: the latest queued frame
  /// with pts <= clock, older ones dropped (frame skipping under a clock
  /// that ran ahead). None while paused, when the due frame is unchanged,
  /// or when decode has not caught up (the caller keeps showing the last
  /// frame; a texture upload only happens on Some).
  pub fn advance(&mut self, clock_us: i64) -> Option<YuvFrame> {
    if !self.playing {
      return None;
    }
    let mut due: Option<YuvFrame> = None;
    loop {
      if self.staged.is_none() {
        match self.frame_rx.try_recv() {
          Ok(frame) => self.staged = Some(frame),
          Err(TryRecvError::Empty) => break,
          Err(TryRecvError::Disconnected) => {
            self.video_done = true;
            break;
          }
        }
      }
      match &self.staged {
        Some(frame) if frame.pts_us <= clock_us => due = self.staged.take(),
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
    self.video_done && self.staged.is_none()
  }
}

// Decodes both streams in pts order into the bounded queues. Blocking sends
// are the backpressure; when the player drops, the sends fail and the worker
// exits. Per-AU decode errors skip the AU (fail-soft playback: openh264 errs
// on B-slices, and one bad AU must not kill the stream).
// The platform decoder: MediaCodec on Android (hardware, handles B-frames),
// openh264 elsewhere (the PoC/dev fallback, B-frame-free content only). No
// software fallback on Android - a creation failure ends the stream, same
// as any decoder-init failure.
#[cfg(target_os = "android")]
fn create_decoder(demux: &Mp4Demuxer) -> Result<Box<dyn VideoDecoder>, String> {
  let info = demux.info();
  let (sps, pps) = demux.parameter_sets();
  Ok(Box::new(super::mediacodec::MediaCodecDecoder::new(info.width, info.height, sps, pps)?))
}

#[cfg(not(target_os = "android"))]
fn create_decoder(_demux: &Mp4Demuxer) -> Result<Box<dyn VideoDecoder>, String> {
  Ok(Box::new(super::h264::H264Decoder::new()?))
}

fn worker(
  demux: &mut Mp4Demuxer,
  audio: Option<&super::demux::AudioInfo>,
  frame_tx: &SyncSender<YuvFrame>,
  pcm_tx: &SyncSender<PcmChunk>,
) {
  let mut decoder = match create_decoder(demux) {
    Ok(d) => d,
    Err(e) => {
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
