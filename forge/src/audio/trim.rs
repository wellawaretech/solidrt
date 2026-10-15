// The one discard rule for decoded Opus audio: samples before a content
// time are dropped. It covers both trims a stream asks for (RFC 7845): the
// priming samples at the start (a packet's first sample sits pre-skip
// before its block time, so the stream's start as the discard point drops
// exactly them) and the preroll after a seek (the packets before the target
// are decoded for the decoder to converge, their output dropped up to the
// target). Shared by video's audio track and the WebM clip decoder.

/// A discard point in content time, cleared once a chunk reaches past it.
pub(crate) struct Trim {
  until_us: Option<i64>,
}

impl Trim {
  /// Discard decoded samples before `until_us` (None discards nothing).
  pub fn new(until_us: Option<i64>) -> Trim {
    Trim { until_us }
  }

  /// A decoded chunk of `frames` frames starting at `pts_us`: how many of
  /// its first frames to drop, and the content time of the first kept
  /// frame. A chunk that crosses the discard point clears it; a chunk
  /// wholly before it is dropped whole (`first == frames`) and leaves it.
  pub fn apply(&mut self, pts_us: i64, frames: usize, sample_rate: u32) -> (usize, i64) {
    let Some(until) = self.until_us else { return (0, pts_us) };
    if pts_us >= until {
      self.until_us = None;
      return (0, pts_us);
    }
    let drop = ((until - pts_us) as i128 * sample_rate as i128 / 1_000_000) as usize;
    let first = drop.min(frames);
    if first < frames {
      self.until_us = None;
    }
    (first, pts_us + (first as i64) * 1_000_000 / sample_rate as i64)
  }
}
