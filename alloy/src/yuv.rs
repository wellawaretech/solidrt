// Planar YUV textures (see okf/backlog/video-playback.md): the layout and
// color vocabulary, the plane geometry of a tightly packed frame, and the
// YUV-to-RGB conversion fragment shader. Video-agnostic by design - any
// producer of packed YUV frames (video decoder, camera) can feed one.
//
// A YUV texture is a composition of existing texture-system primitives, wired
// by Context::create_yuv_texture: registry textures for the planes (R8/RG8,
// two sets, double buffered - see YuvGroup in context/texture.rs) plus a
// shader target that samples them into the app-visible RGBA output. Pixels
// reach it one way: the LATCH below. A producer pushes a packed frame with
// the time it is due on `crate::clock` (a `YuvFrameSink`, usable from any
// thread; a producer with no clock of its own pushes with a due time of
// zero, which latches at the next frame), and at each frame the raster
// thread takes the newest frame due at that frame's presentation deadline,
// uploads it into the back plane set, flips the set and rebinds the
// conversion target, so re-render and content damage propagation are the
// ordinary sampler-graph behavior, nothing YUV-specific. The UI thread
// never carries a frame. The planes being real registry ids is deliberate:
// exposing them (with the color constants) to app shaders is the
// designed-for postprocessing extension.
//
// Frames are TIGHTLY PACKED: plane rows are exactly the plane width, planes
// follow each other with no padding. Producers with padded output (decoder
// stride/slice-height) repack during the copy out of the decoder's buffer,
// where dropping the padding is free. Chroma dimensions round up, so odd
// frame sizes are legal (the last chroma column/row just covers one texel).

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use crate::gpu::TextureFormat;

/// Plane arrangement of a packed YUV 4:2:0 frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvLayout {
  /// Y plane, then one interleaved UV plane at half resolution (the
  /// MediaCodec buffer-mode output on the probed TV).
  Nv12,
  /// Y plane, then U, then V, each chroma plane at half resolution (what
  /// software decoders and some capture sources hand out).
  I420,
}

/// YUV-to-RGB conversion matrix. Absent stream metadata, the convention is
/// BT.709 for HD (720p and up) and BT.601 for SD; the caller decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvMatrix {
  Bt601,
  Bt709,
}

/// Sample range of the Y and chroma values. Video is almost always limited
/// (Y 16..235, chroma 16..240); full uses all 256 steps (JPEG-style).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvRange {
  Limited,
  Full,
}

/// One plane of a packed frame: its texture geometry and format, the uniform
/// the conversion shader samples it under, and its byte offset in the frame.
pub struct YuvPlane {
  pub name: &'static str,
  pub width: u32,
  pub height: u32,
  pub format: TextureFormat,
  pub offset: usize,
}

impl YuvPlane {
  pub fn byte_len(&self) -> usize {
    self.format.byte_len(self.width, self.height)
  }
}

/// The planes of a tightly packed `layout` frame at display size
/// `width` x `height`, in frame order.
pub fn planes(layout: YuvLayout, width: u32, height: u32) -> Vec<YuvPlane> {
  let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
  let y_len = width as usize * height as usize;
  let c_len = cw as usize * ch as usize;
  match layout {
    YuvLayout::Nv12 => vec![
      YuvPlane { name: "uY", width, height, format: TextureFormat::R8, offset: 0 },
      YuvPlane { name: "uUV", width: cw, height: ch, format: TextureFormat::Rg8, offset: y_len },
    ],
    YuvLayout::I420 => vec![
      YuvPlane { name: "uY", width, height, format: TextureFormat::R8, offset: 0 },
      YuvPlane { name: "uU", width: cw, height: ch, format: TextureFormat::R8, offset: y_len },
      YuvPlane { name: "uV", width: cw, height: ch, format: TextureFormat::R8, offset: y_len + c_len },
    ],
  }
}

/// Total byte length of one packed frame.
pub fn frame_size(layout: YuvLayout, width: u32, height: u32) -> usize {
  planes(layout, width, height).iter().map(|p| p.byte_len()).sum()
}

/// The conversion coefficients as plain numbers, for consumers that convert
/// on the CPU or fuse conversion into their own shader (postprocessing
/// tier 2): `[y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u]` where
///   Y' = (y - y_offset) * y_scale,  C = (c - 128/255) * c_scale
///   R = Y' + r_v*Cr,  G = Y' + g_u*Cb + g_v*Cr,  B = Y' + b_u*Cb
pub fn coefficients(matrix: YuvMatrix, range: YuvRange) -> [f32; 7] {
  let (kr, kb) = match matrix {
    YuvMatrix::Bt601 => (0.299f32, 0.114f32),
    YuvMatrix::Bt709 => (0.2126f32, 0.0722f32),
  };
  let kg = 1.0 - kr - kb;
  let (y_scale, y_offset, c_scale) = match range {
    YuvRange::Limited => (255.0 / 219.0, 16.0 / 255.0, 255.0 / 224.0),
    YuvRange::Full => (1.0, 0.0, 1.0),
  };
  let r_v = 2.0 * (1.0 - kr);
  let b_u = 2.0 * (1.0 - kb);
  let g_u = -2.0 * kb * (1.0 - kb) / kg;
  let g_v = -2.0 * kr * (1.0 - kr) / kg;
  [y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u]
}

/// The conversion pass fragment shader for a `layout` frame, color constants
/// baked in (they are fixed per stream; a change of standard recreates the
/// texture). Body-only source for `Context::create_shader_texture`.
pub fn fragment_src(layout: YuvLayout, matrix: YuvMatrix, range: YuvRange) -> String {
  let [y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u] = coefficients(matrix, range);
  let (samplers, chroma) = match layout {
    YuvLayout::Nv12 => ("uniform sampler2D uY;\nuniform sampler2D uUV;", "texture(uUV, vUV).rg"),
    YuvLayout::I420 => (
      "uniform sampler2D uY;\nuniform sampler2D uU;\nuniform sampler2D uV;",
      "vec2(texture(uU, vUV).r, texture(uV, vUV).r)",
    ),
  };
  format!(
    "{samplers}
void main() {{
  float y = (texture(uY, vUV).r - {y_offset:.7}) * {y_scale:.7};
  vec2 c = ({chroma} - vec2({c_off:.7})) * {c_scale:.7};
  vec3 rgb = vec3(y + {r_v:.7} * c.y, y + {g_u:.7} * c.x + {g_v:.7} * c.y, y + {b_u:.7} * c.x);
  fragColor = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}}
",
    c_off = 128.0 / 255.0,
  )
}

// --- The latch: the one way pixels reach a YUV texture ---

// Frames the latch holds waiting to be shown. A producer that leads by one
// refresh period plus its content interval never queues more than this at
// any content rate up to the refresh rate; against a live compositor a
// push past it evicts the oldest (the producer never blocks), against a
// stepped clock the push blocks instead (see `YuvLatchShared::push`).
pub const LATCH_QUEUE_FRAMES: usize = 4;
// How far past a frame's deadline a due time may lie and still be taken
// for that frame, in percent of the refresh period: half a period, so a
// frame belongs to the present whose vsync is closest to its due time (the
// nearest-vsync rule a plane's release snap implements, and the lookahead
// the JS tick measured 2.8% -> 0.07% flips with). Zero in playback, where
// the deadline is the frame's exact virtual time.
pub const LATCH_LOOKAHEAD_PERCENT: i64 = 50;

/// One frame waiting in a latch: its due time on `crate::clock`, its
/// presentation time in the stream, and the packed plane bytes.
pub struct LatchedFrame {
  pub due_ns: i64,
  pub pts_us: i64,
  pub data: Vec<u8>,
}

/// What a take handed back: the frame to show, the queued frames it
/// superseded (older, also due: skipped, never shown), and whether the
/// UI-side peek for the same deadline had concluded nothing was due (a
/// producer that missed its lead; a cached boundary over the texture lags
/// one frame).
pub struct Taken {
  pub frame: LatchedFrame,
  pub skipped: usize,
  pub late: bool,
}

/// The frames waiting to be shown on one YUV texture, ordered by due time,
/// and the take rule against a frame's deadline. Pure state: the sharing
/// (a mutex, the playback waits) is `YuvLatchShared`, and this is what the
/// unit tests exercise.
pub struct YuvLatch {
  queue: VecDeque<LatchedFrame>,
  shown_pts_us: Option<i64>,
  playing: bool,
  ended: bool,
  closed: bool,
  // What the UI-side peek concluded for a deadline: (deadline, due), so
  // the raster's take for the same deadline can tell a late push.
  noted: Option<(i64, bool)>,
  // Frames evicted by a push against a full queue: skipped, never shown.
  evicted: usize,
}

/// What a push did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pushed {
  Queued,
  /// Queued, with the oldest frame evicted to make room.
  Evicted,
  /// No room, and the caller must wait for a take (a stepped clock).
  Full,
  /// The texture is being destroyed: dropped.
  Closed,
}

impl YuvLatch {
  pub fn new() -> YuvLatch {
    YuvLatch {
      queue: VecDeque::new(),
      shown_pts_us: None,
      playing: false,
      ended: false,
      closed: false,
      noted: None,
      evicted: 0,
    }
  }

  /// Queue `frame` in due order. With `evict`, a full queue drops its
  /// oldest frame to make room; without, a full queue refuses (`Full`) and
  /// the caller waits.
  pub fn push(&mut self, frame: LatchedFrame, evict: bool) -> Pushed {
    if self.closed {
      return Pushed::Closed;
    }
    let mut result = Pushed::Queued;
    if self.queue.len() >= LATCH_QUEUE_FRAMES {
      if !evict {
        return Pushed::Full;
      }
      self.queue.pop_front();
      self.evicted += 1;
      result = Pushed::Evicted;
    }
    // Frames arrive in due order; a re-anchor (a seek without a flush, a
    // stall) may put a new frame ahead of queued ones, and the order is
    // what the take rule reads.
    let at = self.queue.iter().rposition(|queued| queued.due_ns <= frame.due_ns).map_or(0, |i| i + 1);
    self.queue.insert(at, frame);
    self.ended = false;
    result
  }

  /// Whether a take for `deadline_ns` would hand a frame back: a frame is
  /// due at or before the deadline plus `lookahead_ns`. Records the answer
  /// for the take to compare against (see `Taken::late`).
  pub fn peek(&mut self, deadline_ns: i64, lookahead_ns: i64) -> bool {
    let due = self.queue.front().is_some_and(|frame| frame.due_ns <= deadline_ns + lookahead_ns);
    self.noted = Some((deadline_ns, due));
    due
  }

  /// Take the newest frame due at or before `deadline_ns` plus
  /// `lookahead_ns`, skipping every older frame that was due too; None
  /// when nothing is. The taken frame's pts becomes `shown_pts_us`.
  pub fn take(&mut self, deadline_ns: i64, lookahead_ns: i64) -> Option<Taken> {
    let limit = deadline_ns + lookahead_ns;
    let mut taken: Option<LatchedFrame> = None;
    let mut skipped = 0;
    while self.queue.front().is_some_and(|frame| frame.due_ns <= limit) {
      if taken.is_some() {
        skipped += 1;
      }
      taken = self.queue.pop_front();
    }
    let frame = taken?;
    // Evictions are reported with the take that follows them; a take that
    // finds nothing due leaves them for the next.
    skipped += self.evicted;
    self.evicted = 0;
    self.shown_pts_us = Some(frame.pts_us);
    let late = self.noted.take().is_some_and(|(noted_ns, due)| noted_ns == deadline_ns && !due);
    Some(Taken { frame, skipped, late })
  }

  /// Whether a take for `deadline_ns` is final: the queue holds a frame due
  /// after the deadline plus `lookahead_ns`, or nothing more will come (the
  /// stream ended, the texture closed, playback stopped). A stepped
  /// consumer waits for this before it takes.
  pub fn settled(&self, deadline_ns: i64, lookahead_ns: i64) -> bool {
    self.ended
      || self.closed
      || !self.playing
      || self.queue.back().is_some_and(|frame| frame.due_ns > deadline_ns + lookahead_ns)
  }

  /// Forget everything queued (a seek).
  pub fn flush(&mut self) {
    self.queue.clear();
    self.evicted = 0;
  }

  pub fn shown_pts_us(&self) -> Option<i64> {
    self.shown_pts_us
  }

  pub fn playing(&self) -> bool {
    self.playing
  }

  pub fn set_playing(&mut self, playing: bool) {
    self.playing = playing;
  }

  pub fn end(&mut self) {
    self.ended = true;
  }

  pub fn close(&mut self) {
    self.closed = true;
    self.queue.clear();
  }

  pub fn closed(&self) -> bool {
    self.closed
  }

  pub fn len(&self) -> usize {
    self.queue.len()
  }

  pub fn is_empty(&self) -> bool {
    self.queue.is_empty()
  }
}

impl Default for YuvLatch {
  fn default() -> Self {
    YuvLatch::new()
  }
}

/// A latch shared between its producer (a worker thread, through a
/// `YuvFrameSink`), the UI thread (the draw gate's peek) and the raster
/// thread (the take at each frame): the state under a mutex, plus the
/// condition the stepped-clock waits sleep on.
pub struct YuvLatchShared {
  state: Mutex<YuvLatch>,
  changed: Condvar,
}

impl YuvLatchShared {
  pub fn new() -> YuvLatchShared {
    YuvLatchShared { state: Mutex::new(YuvLatch::new()), changed: Condvar::new() }
  }

  fn lock(&self) -> MutexGuard<'_, YuvLatch> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Queue a frame. Against a live compositor (`block` false) a full queue
  /// evicts its oldest and the call returns at once; against a stepped
  /// clock (`block` true) it waits for a take to make room, since evicting
  /// would drop a frame the capture has not taken yet. A closed latch
  /// drops the frame.
  pub fn push(&self, frame: LatchedFrame, block: bool) {
    let mut state = self.lock();
    if block {
      while !state.closed() && state.len() >= LATCH_QUEUE_FRAMES {
        state = self.changed.wait(state).unwrap_or_else(PoisonError::into_inner);
      }
    }
    state.push(frame, !block);
    self.changed.notify_all();
  }

  /// The raster's take for a frame with deadline `deadline_ns` (see
  /// `YuvLatch::take`). With `wait`, the take first waits until it is
  /// final (`YuvLatch::settled`), so a captured frame shows the frame due
  /// and never the one that happened to have arrived; it pops what is due
  /// before each wait, so a producer blocked on a full latch of due frames
  /// gets room and the two never wait on each other.
  pub fn take(&self, deadline_ns: i64, lookahead_ns: i64, wait: bool) -> Option<Taken> {
    let mut state = self.lock();
    let mut taken: Option<Taken> = None;
    loop {
      if let Some(newer) = state.take(deadline_ns, lookahead_ns) {
        taken = Some(match taken {
          // The earlier candidate is superseded: one more skipped frame.
          Some(older) => {
            Taken { frame: newer.frame, skipped: older.skipped + 1 + newer.skipped, late: older.late || newer.late }
          }
          None => newer,
        });
        self.changed.notify_all();
      }
      if !wait || state.settled(deadline_ns, lookahead_ns) {
        return taken;
      }
      state = self.changed.wait(state).unwrap_or_else(PoisonError::into_inner);
    }
  }

  pub fn peek(&self, deadline_ns: i64, lookahead_ns: i64) -> bool {
    self.lock().peek(deadline_ns, lookahead_ns)
  }

  pub fn shown_pts_us(&self) -> Option<i64> {
    self.lock().shown_pts_us()
  }

  pub fn playing(&self) -> bool {
    self.lock().playing()
  }

  pub fn set_playing(&self, playing: bool) {
    self.lock().set_playing(playing);
    self.changed.notify_all();
  }

  pub fn end(&self) {
    self.lock().end();
    self.changed.notify_all();
  }

  pub fn flush(&self) {
    self.lock().flush();
    self.changed.notify_all();
  }

  pub fn close(&self) {
    self.lock().close();
    self.changed.notify_all();
  }
}

impl Default for YuvLatchShared {
  fn default() -> Self {
    YuvLatchShared::new()
  }
}

/// The take rule's lookahead now: half the display's refresh period, zero
/// under a stepped clock (playback) or before the rate is known.
pub fn lookahead_ns() -> i64 {
  if crate::clock::stepped() {
    return 0;
  }
  period_ns().map_or(0, |period| period * LATCH_LOOKAHEAD_PERCENT / 100)
}

/// The display's refresh period on the latch's clock, None under a stepped
/// clock (playback has no presentation cadence) or before the rate is
/// known.
pub fn period_ns() -> Option<i64> {
  if crate::clock::stepped() {
    return None;
  }
  crate::refresh_rate().filter(|hz| *hz > 0.0).map(|hz| (1_000_000_000.0 / hz as f64) as i64)
}
