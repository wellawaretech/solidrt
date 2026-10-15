// Opus in WebM as a clip: the WebM reader opened on its audio track alone
// (an audio-only `.webm`/`.weba`, or the sound of a video file), each
// packet through the Opus decoder, the pre-skip and the seek preroll
// trimmed by the shared rule. The same path video's audio track takes,
// without a sink: samples come out on request.

use std::io::{Seek, SeekFrom};

use super::trim::Trim;
use super::{AudioDecoder, AudioFormat, OpusDecoder};
use crate::seek::SeekableReader;
use crate::source::Facts;
use crate::video::{Demuxer, WebmDemuxer};

pub struct WebmAudioDecoder {
  demux: WebmDemuxer,
  decoder: OpusDecoder,
  sample_rate: u32,
  channels: u16,
  frames: Option<u64>,
  // A packet's first sample sits this far before its block time (RFC
  // 7845): the decode time of a packet is its pts less this.
  pre_skip_us: i64,
  trim: Trim,
  // Decoded samples not yet handed out, from `pending_at` on.
  pending: Vec<f32>,
  pending_at: usize,
  cursor: u64,
  ended: bool,
}

impl WebmAudioDecoder {
  /// Open the Opus track of a WebM file from the start of `reader`; errs
  /// when the file is not WebM or has no Opus track.
  pub fn open(mut reader: SeekableReader) -> Result<WebmAudioDecoder, String> {
    let len = reader.seek(SeekFrom::End(0)).map_err(|e| format!("webm: {e}"))?;
    reader.seek(SeekFrom::Start(0)).map_err(|e| format!("webm: {e}"))?;
    let demux = WebmDemuxer::open_audio(reader, Facts { len: Some(len), seekable: true }).map_err(|e| e.to_string())?;
    let info = demux.info().audio.clone().ok_or("webm: no Opus audio track")?;
    let decoder = OpusDecoder::new(info.sample_rate, info.channels)?;
    let frames = demux.info().duration_us.map(|us| (us.max(0) as u128 * info.sample_rate as u128 / 1_000_000) as u64);
    Ok(WebmAudioDecoder {
      demux,
      decoder,
      sample_rate: info.sample_rate,
      channels: info.channels,
      frames,
      pre_skip_us: info.pre_skip as i64 * 1_000_000 / info.sample_rate as i64,
      trim: Trim::new(Some(0)),
      pending: Vec::new(),
      pending_at: 0,
      cursor: 0,
      ended: false,
    })
  }

  /// Decode packets until some samples are pending or the track ends.
  fn refill(&mut self) -> Result<(), String> {
    while self.pending_at >= self.pending.len() && !self.ended {
      let Some(packet) = self.demux.next_audio().map_err(|e| e.to_string())? else {
        self.ended = true;
        return Ok(());
      };
      let chunk = match self.decoder.decode(packet.pts_us - self.pre_skip_us, &packet.data) {
        Ok(chunk) => chunk,
        Err(e) => {
          log::warn!("[forge::audio] webm: skipping audio packet at {}us: {e}", packet.pts_us);
          continue;
        }
      };
      let frames = chunk.samples.len() / self.channels as usize;
      let (first, _) = self.trim.apply(chunk.pts_us, frames, self.sample_rate);
      let last = frames - padding_frames(packet.discard_padding_us, self.sample_rate).min(frames);
      if first >= last {
        continue;
      }
      self.pending = chunk.samples;
      self.pending.truncate(last * self.channels as usize);
      self.pending_at = first * self.channels as usize;
    }
    Ok(())
  }
}

/// The frames a packet's DiscardPadding covers at `sample_rate`.
pub(crate) fn padding_frames(discard_padding_us: i64, sample_rate: u32) -> usize {
  (discard_padding_us.max(0) as i128 * sample_rate as i128 / 1_000_000) as usize
}

impl AudioDecoder for WebmAudioDecoder {
  fn format(&self) -> AudioFormat {
    AudioFormat { sample_rate: self.sample_rate, channels: self.channels, frames: self.frames }
  }

  fn read(&mut self, out: &mut [f32]) -> Result<usize, String> {
    let channels = self.channels as usize;
    let room = out.len() / channels * channels;
    let mut filled = 0usize;
    while filled < room {
      self.refill()?;
      let available = &self.pending[self.pending_at..];
      if available.is_empty() {
        break;
      }
      let take = available.len().min(room - filled);
      out[filled..filled + take].copy_from_slice(&available[..take]);
      self.pending_at += take;
      filled += take;
    }
    let frames = filled / channels;
    self.cursor += frames as u64;
    Ok(frames)
  }

  fn seek(&mut self, frame: u64) -> Result<(), String> {
    let target_us = (frame as i128 * 1_000_000 / self.sample_rate as i128) as i64;
    // The reader delivers the preroll packets before the target; decoding
    // them from a fresh state converges the decoder, and the trim drops
    // their output up to the target.
    self.demux.seek(target_us).map_err(|e| e.to_string())?;
    self.decoder.reset();
    self.trim = Trim::new(Some(target_us));
    self.pending.clear();
    self.pending_at = 0;
    self.cursor = frame;
    self.ended = false;
    Ok(())
  }
}
