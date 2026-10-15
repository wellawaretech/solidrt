// The audio track of a playing stream: Opus packets from the demuxer,
// decoded and pushed into the caller's sink ahead of playback, with the two
// trims the stream asks for (pre-skip at the start, preroll after a seek)
// done by the shared discard rule (`audio::trim`), and the sink's position
// mapped back to content time for the transport's clock correction.
// Engine-free and sink-agnostic (`AudioSink` is the caller's).

use super::reader::Next;
use super::transport::AudioSink;
use super::{AudioInfo, AudioPacket, OpusDecoder};
use crate::audio::trim::Trim;
use crate::audio::webm::padding_frames;

// How much decoded audio the sink holds ahead of playback. Small enough to
// keep pause and seek latency low (a paused sink holds its queue, a seek
// clears it), large enough to ride out the longest wait a player's worker
// makes between top-ups (a frame's release lead plus one output dequeue).
pub const AUDIO_LOOKAHEAD_US: i64 = 500_000;
// How much later the sound of a content instant is heard than its picture
// is seen when both leave the player together, net of the two output
// paths: the sound's (SDL's device buffer, the platform mixer, the TV's
// speakers or an HDMI/Bluetooth path) minus the picture's (a released
// frame scans out about two vsyncs later). The plane player holds the
// picture back by this much when the sound starts (a negative value holds
// the sound back, by advancing the picture), and the sink position is read
// back through it. Not measurable from here (SDL exposes no output
// timestamp), so set with a flash-and-beep clip (examples/video/assets/
// avsync.webm): 60 ms lands the beep on the flash on the Philips TV's own
// speakers (2026-09-13; 20 ms left the sound visibly behind). An external
// speaker path needs its own value, which is an app setting to come.
pub const AUDIO_OUTPUT_LATENCY_US: i64 = 60_000;

pub struct AudioTrack {
  decoder: OpusDecoder,
  sink: Box<dyn AudioSink>,
  sample_rate: u32,
  channels: u16,
  pre_skip_us: i64,
  // Decoded samples before the stream's start (pre-skip) or a seek target
  // (preroll) are discarded.
  trim: Trim,
  // Content time <-> sink position, set at every push: this content time
  // starts playing when the sink's position reaches this value.
  base: Option<(i64, i64)>,
  done: bool,
}

impl AudioTrack {
  /// A track feeding `sink` (opened at the stream's rate and channels),
  /// positioned at `start_us` (the stream's `MediaInfo::start_us`):
  /// decoded samples before it are discarded, the pre-skip among them.
  pub fn new(info: &AudioInfo, start_us: i64, sink: Box<dyn AudioSink>) -> Result<AudioTrack, String> {
    let decoder = OpusDecoder::new(info.sample_rate, info.channels)?;
    let pre_skip_us = info.pre_skip as i64 * 1_000_000 / info.sample_rate as i64;
    Ok(AudioTrack {
      decoder,
      sink,
      sample_rate: info.sample_rate,
      channels: info.channels,
      pre_skip_us,
      trim: Trim::new(Some(start_us.max(0))),
      base: None,
      done: false,
    })
  }

  /// Decode and push the packets `next` hands out until the sink holds the
  /// lookahead, the source has nothing yet, or the track ends. Cheap when
  /// the sink is full: one queue read.
  pub fn feed(&mut self, mut next: impl FnMut() -> Next<AudioPacket>) {
    while !self.done && self.sink.queued_us() < AUDIO_LOOKAHEAD_US {
      let packet = match next() {
        Next::Packet(packet) => packet,
        Next::Waiting => return,
        Next::End => {
          self.done = true;
          // The sink plays out what it holds, then sits silent (not
          // starved) until a seek clears and refills it.
          self.sink.end();
          return;
        }
      };
      // A packet's first sample is pre-skip before its block time: the
      // block times count from the first shown sample (RFC 7845), so the
      // stream's priming samples fall before content time 0 and the start
      // rule discards them like any other pre-target output.
      let start_us = packet.pts_us - self.pre_skip_us;
      let chunk = match self.decoder.decode(start_us, &packet.data) {
        Ok(chunk) => chunk,
        Err(e) => {
          log::warn!("[forge::video] skipping audio packet at {}us: {e}", packet.pts_us);
          continue;
        }
      };
      let frames = chunk.samples.len() / self.channels as usize;
      let (first, pts_us) = self.trim.apply(chunk.pts_us, frames, self.sample_rate);
      // The stream's last packet ends in encoder padding the container
      // says to discard.
      let last = frames - padding_frames(packet.discard_padding_us, self.sample_rate).min(frames);
      if first >= last {
        continue;
      }
      let samples = &chunk.samples[first * self.channels as usize..last * self.channels as usize];
      // Where this chunk lands in the sink: after everything queued now.
      let starts_at = self.sink.position_us() + self.sink.queued_us();
      if let Err(e) = self.sink.push(samples) {
        log::warn!("[forge::video] {e}");
        self.done = true;
        return;
      }
      self.base = Some((pts_us, starts_at));
    }
  }

  /// Pause or resume the sink with the picture: paused, the queue holds
  /// and the position freezes. (The plane player's; the texture player
  /// pauses its sink from the frame loop.)
  #[cfg_attr(not(target_os = "android"), allow(dead_code))]
  pub fn set_playing(&mut self, playing: bool) {
    self.sink.set_paused(!playing);
  }

  /// Microseconds of decoded audio the sink holds ahead of the device.
  #[cfg_attr(not(target_os = "android"), allow(dead_code))]
  pub fn queued_us(&self) -> i64 {
    self.sink.queued_us()
  }

  /// The track has ended: the stream's audio is over, or the sink refused
  /// a push. Nothing more will be queued until a seek.
  pub fn ended(&self) -> bool {
    self.done
  }

  /// Microseconds of audio the sink has consumed: it moves once the device
  /// pulls, which the plane player waits for before anchoring the picture.
  #[cfg_attr(not(target_os = "android"), allow(dead_code))]
  pub fn sink_position_us(&self) -> i64 {
    self.sink.position_us()
  }

  /// Restart at `target_us`: queued audio goes, the decoder forgets its
  /// state, and the preroll packets the demuxer now delivers are decoded
  /// with their output discarded up to the target.
  pub fn seek(&mut self, target_us: i64) {
    self.sink.clear();
    self.decoder.reset();
    self.trim = Trim::new(Some(target_us.max(0)));
    self.base = None;
    self.done = false;
  }

  /// The content time the speaker is at, from the sink's position; None
  /// until something has been pushed.
  pub fn content_time_us(&self) -> Option<i64> {
    let (content_us, position_us) = self.base?;
    Some(content_us + (self.sink.position_us() - position_us) - AUDIO_OUTPUT_LATENCY_US)
  }
}
