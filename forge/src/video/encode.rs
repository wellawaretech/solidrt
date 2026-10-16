// Encoding: the configuration, the packet, the contract every video encoder
// backend meets (libvpx off Android, AMediaCodec on it: see
// okf/plans/video-encode.md) and the RGBA to I420 conversion a source's
// frames take on the way in. The recorder (record.rs) drives one encoder
// per clip; nothing here knows a container or a thread.

use super::PixelLayout;

/// What the clip is for: the rate-control mode follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeMode {
  /// A file: one-pass VBR, quality over a steady rate.
  Recording,
  /// A link: CBR and error-resilient frames, for a receiver that may lose
  /// a packet.
  Live,
}

#[derive(Clone, Debug)]
pub struct EncoderConfig {
  pub width: u32,
  pub height: u32,
  /// The nominal rate the rate control budgets for; frames carry their
  /// own timestamps.
  pub frame_rate: u32,
  pub bitrate_bps: u32,
  /// The longest run between keyframes (a seek lands on one, a joining
  /// viewer waits for one).
  pub keyframe_interval_us: i64,
  pub mode: EncodeMode,
}

/// One coded frame: what a WebM SimpleBlock holds.
#[derive(Clone, Debug)]
pub struct EncodedFrame {
  pub pts_us: i64,
  pub keyframe: bool,
  pub data: Vec<u8>,
}

/// A synchronous video encoder: one tightly packed I420 frame in (the
/// `PixelLayout::I420` contract), zero or more packets out, `flush` at the
/// end for the remainder. The bitrate may change between frames.
pub trait VideoEncoder: Send {
  fn encode(&mut self, i420: &[u8], pts_us: i64, force_keyframe: bool) -> Result<Vec<EncodedFrame>, String>;
  fn flush(&mut self) -> Result<Vec<EncodedFrame>, String>;
  fn set_bitrate(&mut self, bitrate_bps: u32) -> Result<(), String>;
}

/// The platform's VP9 encoder for `config`.
#[cfg(not(target_os = "android"))]
pub fn open_encoder(config: &EncoderConfig) -> Result<Box<dyn VideoEncoder>, String> {
  Ok(Box::new(super::vpx::Vp9Encoder::new(config)?))
}

/// The platform's VP9 encoder for `config`: AMediaCodec's, hardware where
/// the device has one.
#[cfg(target_os = "android")]
pub fn open_encoder(config: &EncoderConfig) -> Result<Box<dyn VideoEncoder>, String> {
  Ok(Box::new(super::mediacodec::encode::MediaCodecEncoder::new(config)?))
}

// BT.601 studio-range RGB to YCbCr, fixed point with eight fractional bits
// (the ITU coefficients times 256, rounded): Y in 16..235, chroma in
// 16..240 around 128. The matrix the track's Colour element names, so a
// player converts back with the same one.
const Y_R: i32 = 66;
const Y_G: i32 = 129;
const Y_B: i32 = 25;
const U_R: i32 = -38;
const U_G: i32 = -74;
const U_B: i32 = 112;
const V_R: i32 = 112;
const V_G: i32 = -94;
const V_B: i32 = -18;
const Y_OFFSET: i32 = 16;
const CHROMA_OFFSET: i32 = 128;
const FIXED_SHIFT: u32 = 8;
const FIXED_HALF: i32 = 1 << (FIXED_SHIFT - 1);
const RGBA_BYTES: usize = 4;

/// Convert a tightly packed RGBA8 frame (the camera tap's and the window
/// tap's layout) into tightly packed I420 in `out`, replacing its
/// contents. Chroma is the mean of each 2x2 block; an odd edge repeats its
/// last row or column. Errs when `rgba` is not `width` x `height` pixels.
pub fn rgba_to_i420(rgba: &[u8], width: u32, height: u32, out: &mut Vec<u8>) -> Result<(), String> {
  let (w, h) = (width as usize, height as usize);
  if rgba.len() != w * h * RGBA_BYTES {
    return Err(format!("frame of {} bytes is not {width}x{height} RGBA", rgba.len()));
  }
  let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
  out.clear();
  out.resize(PixelLayout::I420.frame_size(width, height), 0);
  let (y_plane, chroma) = out.split_at_mut(w * h);
  let (u_plane, v_plane) = chroma.split_at_mut(cw * ch);

  for (row, y_row) in y_plane.chunks_exact_mut(w.max(1)).enumerate() {
    let src = &rgba[row * w * RGBA_BYTES..(row + 1) * w * RGBA_BYTES];
    for (px, y) in src.chunks_exact(RGBA_BYTES).zip(y_row.iter_mut()) {
      let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
      *y = (((Y_R * r + Y_G * g + Y_B * b + FIXED_HALF) >> FIXED_SHIFT) + Y_OFFSET) as u8;
    }
  }
  for cy in 0..ch {
    let rows = [2 * cy, (2 * cy + 1).min(h - 1)];
    for cx in 0..cw {
      let cols = [2 * cx, (2 * cx + 1).min(w - 1)];
      let (mut u, mut v) = (0i32, 0i32);
      for row in rows {
        for col in cols {
          let at = (row * w + col) * RGBA_BYTES;
          let (r, g, b) = (rgba[at] as i32, rgba[at + 1] as i32, rgba[at + 2] as i32);
          u += U_R * r + U_G * g + U_B * b;
          v += V_R * r + V_G * g + V_B * b;
        }
      }
      // Four samples: the mean before the fixed-point shift keeps the
      // rounding to one step.
      u_plane[cy * cw + cx] = (((u / 4 + FIXED_HALF) >> FIXED_SHIFT) + CHROMA_OFFSET) as u8;
      v_plane[cy * cw + cx] = (((v / 4 + FIXED_HALF) >> FIXED_SHIFT) + CHROMA_OFFSET) as u8;
    }
  }
  Ok(())
}
