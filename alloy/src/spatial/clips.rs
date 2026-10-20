// Clip data: baked keyframe channels (the glTF triple of step, linear and
// cubic Hermite over position, rotation, scale and morph weights, plus
// named float curves), the sampler every player reads through, the time
// markers a player fires as it crosses them, and the additive derivation
// that turns an authored clip into deltas against a reference pose. A
// clip is shared data: a channel's `target_slot` indexes the joints of the
// rig a player binds it to (rig.rs), so one clip drives any number of
// instances and retargeting is another rig. Engine independent: native
// types in and out (rendertree rules).

use std::collections::HashMap;

use super::transitions::{quat_normalize, slerp};

pub type ClipId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelPath {
  Position,
  Rotation,
  Scale,
  /// The joint's weights register (morph target weights): one element per
  /// target, the count the channel's data fixes.
  Weights,
  /// A named float curve (Unreal's animation curves): `target_slot`
  /// indexes the clip's curve names, not the rig's joints; one element.
  Curve,
}

impl ChannelPath {
  /// The element count the path dictates: 3, 4 for a rotation, 1 for a
  /// curve, none for weights (as many as the mesh has targets).
  pub(super) fn fixed_elements(self) -> Option<usize> {
    match self {
      ChannelPath::Rotation => Some(4),
      ChannelPath::Weights => None,
      ChannelPath::Curve => Some(1),
      _ => Some(3),
    }
  }

  /// Whether the path addresses a joint (a rig slot) or a curve.
  pub(super) fn on_joint(self) -> bool {
    self != ChannelPath::Curve
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelInterpolation {
  Step,
  Linear,
  Cubic,
}

/// One animated property of one target: the baked keys. `target_slot`
/// indexes the rig's joint table (a curve channel: the clip's curve names).
#[derive(Clone, Debug)]
pub struct ClipChannel {
  pub target_slot: u32,
  pub path: ChannelPath,
  pub interpolation: ChannelInterpolation,
  /// Floats per key value: 3, 4 for a rotation, one per morph target for
  /// weights, 1 for a curve.
  pub elements: usize,
  /// Key times in seconds, ascending.
  pub times: Vec<f32>,
  /// `elements` floats per key; cubic stores three elements per key
  /// (in-tangent, value, out-tangent).
  pub values: Vec<f32>,
}

/// A time marker on a clip (Unity's AnimationEvent, Unreal's notify,
/// Godot's marker): fired when a player crosses `time`. With a
/// `duration` above 0 it is a window (Unreal's notify state): `Begin`
/// fires on entering, `End` on leaving, and on a stop inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipMarker {
  pub time: f64,
  pub duration: f64,
  pub name: String,
}

/// The phase a marker event reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerPhase {
  Instant,
  Begin,
  End,
}

pub(super) struct Clip {
  pub(super) duration: f64,
  pub(super) channels: Vec<ClipChannel>,
  /// The curve names the `Curve` channels' slots index.
  pub(super) curves: Vec<String>,
  /// Ascending by time.
  pub(super) markers: Vec<ClipMarker>,
}

impl Clip {
  /// The channel on `slot` along `path`, if the clip animates it (the
  /// first one; a clip animating one property twice is a baker's error).
  pub(super) fn channel_for(&self, slot: u32, path: ChannelPath) -> Option<&ClipChannel> {
    self.channels.iter().find(|c| c.target_slot == slot && c.path == path)
  }
}

/// The clip registry (a Spatial field; the public API is the Spatial
/// methods in rig.rs).
#[derive(Default)]
pub(super) struct ClipSet {
  pub(super) clips: HashMap<ClipId, Clip>,
  pub(super) next: ClipId,
}

/// Floats per key and the value's offset within a key (cubic keys are
/// [in, value, out]).
pub(super) fn key_layout(channel: &ClipChannel) -> (usize, usize) {
  let elements = channel.elements;
  if channel.interpolation == ChannelInterpolation::Cubic {
    (elements * 3, elements)
  } else {
    (elements, 0)
  }
}

/// Sample one channel at `time` (seconds, clamped to the key range) into
/// `out[..elements]` (`out` at least `channel.elements` long). `cursor`
/// caches the last lo key index; a time at or past it advances linearly,
/// a time before it (seek, loop wrap) falls back to binary search. The
/// keys are stored f32 (baked data, joint-local values); the sample comes
/// out in the arena's f64.
pub(crate) fn sample(channel: &ClipChannel, time: f64, cursor: &mut u32, out: &mut [f64]) {
  let times = &channel.times;
  let values = &channel.values;
  let keys = times.len();
  let elements = channel.elements;
  let (stride, mid) = key_layout(channel);
  if keys == 0 {
    return;
  }
  let copy = |out: &mut [f64], at: usize| {
    for e in 0..elements {
      out[e] = values[at + e] as f64;
    }
  };
  if time <= times[0] as f64 || keys == 1 {
    copy(out, mid);
    return;
  }
  if time >= times[keys - 1] as f64 {
    copy(out, (keys - 1) * stride + mid);
    return;
  }
  // The key pair around `time`: the last key at or before it.
  let mut lo = (*cursor as usize).min(keys - 2);
  if times[lo] as f64 > time {
    let mut a = 0;
    let mut b = lo;
    while b - a > 1 {
      let m = (a + b) >> 1;
      if times[m] as f64 <= time {
        a = m;
      } else {
        b = m;
      }
    }
    lo = a;
  } else {
    // time < times[keys - 1] bounds the walk.
    while times[lo + 1] as f64 <= time {
      lo += 1;
    }
  }
  *cursor = lo as u32;
  let hi = lo + 1;
  let t0 = times[lo] as f64;
  let t1 = times[hi] as f64;
  let span = t1 - t0;
  let s = if span > 0.0 { (time - t0) / span } else { 0.0 };
  let a = lo * stride + mid;
  let b = hi * stride + mid;
  match channel.interpolation {
    ChannelInterpolation::Step => copy(out, a),
    ChannelInterpolation::Linear => {
      if channel.path == ChannelPath::Rotation {
        let qa = [values[a] as f64, values[a + 1] as f64, values[a + 2] as f64, values[a + 3] as f64];
        let qb = [values[b] as f64, values[b + 1] as f64, values[b + 2] as f64, values[b + 3] as f64];
        out[..4].copy_from_slice(&slerp(qa, qb, s));
      } else {
        for e in 0..elements {
          let (va, vb) = (values[a + e] as f64, values[b + e] as f64);
          out[e] = va + (vb - va) * s;
        }
      }
    }
    ChannelInterpolation::Cubic => {
      // glTF's Hermite: p(s) = h00 v0 + h10 d b0 + h01 v1 + h11 d a1,
      // where b0 is key lo's OUT-tangent and a1 key hi's IN-tangent
      // (per-second, so they scale by the span d).
      let s2 = s * s;
      let s3 = s2 * s;
      let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
      let h10 = s3 - 2.0 * s2 + s;
      let h01 = -2.0 * s3 + 3.0 * s2;
      let h11 = s3 - s2;
      let out_tan = lo * stride + elements * 2;
      let in_tan = hi * stride;
      for e in 0..elements {
        out[e] = h00 * values[a + e] as f64
          + h10 * span * values[out_tan + e] as f64
          + h01 * values[b + e] as f64
          + h11 * span * values[in_tan + e] as f64;
      }
      if channel.path == ChannelPath::Rotation {
        let q = quat_normalize([out[0], out[1], out[2], out[3]]);
        out[..4].copy_from_slice(&q);
      }
    }
  }
}

/// Validate a channel's shape against its path (the one indexing safety
/// boundary; key-time ordering is the baker's contract).
pub(super) fn check_channel(i: usize, c: &ClipChannel) -> Result<(), String> {
  let keys = c.times.len();
  if keys == 0 {
    return Err(format!("clip channel {i} has no keys"));
  }
  match c.path.fixed_elements() {
    Some(fixed) if c.elements != fixed => {
      return Err(format!("clip channel {i} has {} elements per key, its path takes {fixed}", c.elements));
    }
    None if c.elements == 0 => return Err(format!("clip channel {i} is a weights channel with no targets")),
    _ => {}
  }
  let (stride, _) = key_layout(c);
  if c.values.len() != keys * stride {
    return Err(format!(
      "clip channel {i} has {} keys but {} values ({} per key expected)",
      keys,
      c.values.len(),
      stride
    ));
  }
  if !c.times.windows(2).all(|w| w[0] <= w[1]) {
    return Err(format!("clip channel {i} has descending key times"));
  }
  Ok(())
}

/// Hamilton product a * b for [x, y, z, w] quaternions.
pub(super) fn quat_multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
  let (ax, ay, az, aw) = (a[0], a[1], a[2], a[3]);
  let (bx, by, bz, bw) = (b[0], b[1], b[2], b[3]);
  [
    aw * bx + ax * bw + ay * bz - az * by,
    aw * by - ax * bz + ay * bw + az * bx,
    aw * bz + ax * by - ay * bx + az * bw,
    aw * bw - ax * bx - ay * by - az * bz,
  ]
}

/// The conjugate of a unit quaternion: its inverse.
pub(super) fn quat_inverse(q: [f64; 4]) -> [f64; 4] {
  [-q[0], -q[1], -q[2], q[3]]
}

/// The additive form of one channel against a reference value: the key
/// values become deltas such that `apply(reference, delta)` gives the
/// authored key back - position and weights subtract, a rotation is
/// left-multiplied by the reference's inverse (`pose = ref * delta`
/// in the joint's own frame), a scale divides (a zero reference
/// component passes the authored scale through). Cubic tangents transform
/// by the same linear map where there is one (a rotation's and a scale's),
/// and stay as authored where the map is a translation (a subtraction
/// leaves a slope alone). The reference is `reference[..elements]`.
pub(super) fn additive_channel(channel: &ClipChannel, reference: &[f64]) -> ClipChannel {
  let elements = channel.elements;
  let (stride, mid) = key_layout(channel);
  let cubic = channel.interpolation == ChannelInterpolation::Cubic;
  let mut values = channel.values.clone();
  let keys = channel.times.len();
  for k in 0..keys {
    let base = k * stride;
    match channel.path {
      ChannelPath::Rotation => {
        let inv = quat_inverse(quat_normalize([reference[0], reference[1], reference[2], reference[3]]));
        let parts: &[usize] = if cubic { &[0, 1, 2] } else { &[0] };
        for &part in parts {
          let at = base + part * elements;
          let q = [values[at] as f64, values[at + 1] as f64, values[at + 2] as f64, values[at + 3] as f64];
          let d = quat_multiply(inv, q);
          for (v, x) in values[at..at + 4].iter_mut().zip(d) {
            *v = x as f32;
          }
        }
      }
      ChannelPath::Scale => {
        let parts: &[usize] = if cubic { &[0, 1, 2] } else { &[0] };
        for &part in parts {
          let at = base + part * elements;
          for e in 0..elements {
            if reference[e] != 0.0 {
              values[at + e] = (values[at + e] as f64 / reference[e]) as f32;
            }
          }
        }
      }
      _ => {
        let at = base + mid;
        for e in 0..elements {
          values[at + e] = (values[at + e] as f64 - reference[e]) as f32;
        }
      }
    }
  }
  ClipChannel { values, ..channel.clone() }
}

/// The marker events a player's step from `prev` to `now` fires, in
/// time order, appended to `out` as (marker index, phase). `forward`
/// is the sign of the step (reverse play fires in reverse), `wrap` is the
/// window's [start, end] when the step wrapped around it (the markers
/// between `prev` and the end fire, then those from the start to `now`),
/// `inclusive` fires a marker sitting exactly at `prev` (a player's first
/// advance, or the one after a seek). Windows (duration > 0) report
/// their edges as `Begin` and `End` by the direction they are crossed
/// in: entering is Begin whichever edge it is, leaving is End; `inside`
/// (one flag per marker) keeps a window from reporting twice and is
/// updated here. Markers outside `window` never fire.
#[allow(clippy::too_many_arguments)]
pub(crate) fn crossings(
  markers: &[ClipMarker],
  prev: f64,
  now: f64,
  forward: bool,
  wrap: Option<(f64, f64)>,
  window: (f64, f64),
  inclusive: bool,
  inside: &mut [bool],
  out: &mut Vec<(usize, MarkerPhase)>,
) {
  // A step is one or two monotone segments.
  let mut segments: [Option<(f64, f64, bool)>; 2] = [None, None];
  match wrap {
    None => segments[0] = Some((prev, now, inclusive)),
    Some((start, end)) => {
      if forward {
        segments[0] = Some((prev, end, inclusive));
        segments[1] = Some((start, now, true));
      } else {
        segments[0] = Some((prev, start, inclusive));
        segments[1] = Some((end, now, true));
      }
    }
  }
  // Every edge crossed in a segment, then the segment's edges in the
  // direction of travel. Instants have one edge; a window's two edges
  // are entering and leaving by the direction crossed.
  let mut edges: Vec<(f64, usize, MarkerPhase)> = Vec::new();
  for segment in segments.iter().flatten() {
    let (from, to, incl) = *segment;
    edges.clear();
    for (i, m) in markers.iter().enumerate() {
      if m.time < window.0 || m.time > window.1 {
        continue;
      }
      let crossed = |t: f64| -> bool {
        if forward {
          (from < t || (incl && from == t)) && t <= to
        } else {
          (t < from || (incl && from == t)) && to <= t
        }
      };
      if m.duration <= 0.0 {
        if crossed(m.time) {
          edges.push((m.time, i, MarkerPhase::Instant));
        }
        continue;
      }
      let close = (m.time + m.duration).min(window.1);
      let (enter, leave) = if forward { (m.time, close) } else { (close, m.time) };
      if crossed(enter) {
        edges.push((enter, i, MarkerPhase::Begin));
      }
      if crossed(leave) {
        edges.push((leave, i, MarkerPhase::End));
      }
    }
    if forward {
      edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    } else {
      edges.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    }
    for &(_, i, phase) in &edges {
      match phase {
        MarkerPhase::Instant => out.push((i, phase)),
        MarkerPhase::Begin => {
          if !inside[i] {
            inside[i] = true;
            out.push((i, phase));
          }
        }
        MarkerPhase::End => {
          if inside[i] {
            inside[i] = false;
            out.push((i, phase));
          }
        }
      }
    }
  }
}
