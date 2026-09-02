// Clip players: baked keyframe channels evaluated in core, writing node
// TRS through the ordinary snap path once per frame BEFORE the frame's
// JS - so app code (root-motion strips, skeleton copies, aiming) reads
// and overwrites freshly posed nodes, last write wins, and the draw
// path's single flush publishes everything (uModel, palettes, picking)
// as usual. JS keeps policy at O(changes): play/stop/crossfade are a
// player create and a couple of weight-fade writes; sampling and
// blending are per-frame native work.
//
// The blend is the 3d mixer's contract, generalized: per (node, path),
// the weighted average over the players animating it - incremental, each
// contributor slerping/lerping in by its share of the accumulated
// weight - so two players on one node crossfade and players on disjoint
// nodes are independent. Sampling is the glTF triple (step, linear,
// cubic Hermite; rotations slerp the short arc, cubic renormalizes per
// spec), with a per-channel cursor so sequential playback finds its key
// pair in O(1) and only a seek or loop wrap pays the binary search.

use std::collections::HashMap;

use super::transitions::{quat_normalize, slerp};
use super::{NodeId, Spatial};

pub type ClipId = u64;
pub type PlayerId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelPath {
  Position,
  Rotation,
  Scale,
}

impl ChannelPath {
  fn elements(self) -> usize {
    match self {
      ChannelPath::Rotation => 4,
      _ => 3,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelInterpolation {
  Step,
  Linear,
  /// glTF CUBICSPLINE: values store [in-tangent, value, out-tangent] per
  /// key, tangents per second.
  Cubic,
}

/// One baked track: keys over one path of one target slot. `target_slot`
/// indexes a player's target table (clips are shared data; the table is
/// the per-instance binding, which is also what retargeting swaps).
pub struct ClipChannel {
  pub target_slot: u32,
  pub path: ChannelPath,
  pub interpolation: ChannelInterpolation,
  /// Key times in seconds, ascending.
  pub times: Vec<f32>,
  /// `elements` floats per key (3, or 4 for rotation); cubic stores three
  /// elements per key.
  pub values: Vec<f32>,
}

struct Clip {
  duration: f64,
  channels: Vec<ClipChannel>,
}

/// One playing clip instance: the mixer's Action, native. `fade` is the
/// weight change per second (positive in, negative out, 0 steady); a
/// player fading past 0 is removed (a Dropped event), one reaching 1
/// stops fading. A non-looping player reaching its end holds the final
/// pose and reports Finished once.
struct Player {
  id: PlayerId,
  clip: ClipId,
  targets: Vec<NodeId>,
  time: f64,
  speed: f32,
  weight: f32,
  fade: f32,
  looped: bool,
  finished: bool,
  /// Last key-pair lo index per channel (the sequential-playback cache).
  cursors: Vec<u32>,
}

/// A partial player write: the O(changes) control channel. `time` also
/// clears `finished` (a scrub back re-arms the end report).
#[derive(Default)]
pub struct PlayerUpdate {
  pub weight: Option<f32>,
  pub fade: Option<f32>,
  pub speed: Option<f32>,
  pub time: Option<f64>,
}

/// Why a player left the set (drained by the embedder after each advance
/// and delivered to JS as one engine event per entry). `Finished` fires
/// once when a non-looping player reaches its end - the player STAYS,
/// holding the pose. `Dropped` fires when one is removed without
/// finishing: faded out, or its clip or a target node was destroyed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipEvent {
  Finished(PlayerId),
  Dropped(PlayerId),
}

/// What an advance produced: `active` is standing frame demand (a player
/// that can still progress), `wrote` means node TRS changed this advance
/// (this frame must flush and paint).
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayersTick {
  pub active: bool,
  pub wrote: bool,
}

/// The clip registry and player set (a Spatial field; the public API is
/// the Spatial methods below).
#[derive(Default)]
pub(super) struct PlayerSet {
  clips: HashMap<ClipId, Clip>,
  players: Vec<Player>,
  next_clip: ClipId,
  next_player: PlayerId,
  last_ms: f64,
  events: Vec<ClipEvent>,
  /// Blend accumulator, keyed (node, path index), reused across frames.
  slots: HashMap<(NodeId, u8), BlendSlot>,
}

struct BlendSlot {
  sum: f32,
  value: [f32; 4],
}

/// Sample one channel at `time` (seconds, clamped to the key range) into
/// `out[..elements]`. `cursor` caches the last lo key index; a time at or
/// past it advances linearly, a time before it (seek, loop wrap) falls
/// back to binary search.
pub(crate) fn sample(channel: &ClipChannel, time: f32, cursor: &mut u32, out: &mut [f32; 4]) {
  let times = &channel.times;
  let values = &channel.values;
  let keys = times.len();
  let elements = channel.path.elements();
  let cubic = channel.interpolation == ChannelInterpolation::Cubic;
  let stride = if cubic { elements * 3 } else { elements };
  // The value element offset within a key: cubic keys are [in, value, out].
  let mid = if cubic { elements } else { 0 };
  if keys == 0 {
    return;
  }
  if time <= times[0] || keys == 1 {
    out[..elements].copy_from_slice(&values[mid..mid + elements]);
    return;
  }
  if time >= times[keys - 1] {
    let at = (keys - 1) * stride + mid;
    out[..elements].copy_from_slice(&values[at..at + elements]);
    return;
  }
  // The key pair around `time`: the last key at or before it.
  let mut lo = (*cursor as usize).min(keys - 2);
  if times[lo] > time {
    let mut a = 0;
    let mut b = lo;
    while b - a > 1 {
      let m = (a + b) >> 1;
      if times[m] <= time {
        a = m;
      } else {
        b = m;
      }
    }
    lo = a;
  } else {
    // time < times[keys - 1] bounds the walk.
    while times[lo + 1] <= time {
      lo += 1;
    }
  }
  *cursor = lo as u32;
  let hi = lo + 1;
  let t0 = times[lo];
  let t1 = times[hi];
  let span = t1 - t0;
  let s = if span > 0.0 { (time - t0) / span } else { 0.0 };
  let a = lo * stride + mid;
  let b = hi * stride + mid;
  match channel.interpolation {
    ChannelInterpolation::Step => out[..elements].copy_from_slice(&values[a..a + elements]),
    ChannelInterpolation::Linear => {
      if channel.path == ChannelPath::Rotation {
        let qa = [values[a], values[a + 1], values[a + 2], values[a + 3]];
        let qb = [values[b], values[b + 1], values[b + 2], values[b + 3]];
        out.copy_from_slice(&slerp(qa, qb, s));
      } else {
        for e in 0..elements {
          out[e] = values[a + e] + (values[b + e] - values[a + e]) * s;
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
        out[e] =
          h00 * values[a + e] + h10 * span * values[out_tan + e] + h01 * values[b + e] + h11 * span * values[in_tan + e];
      }
      if channel.path == ChannelPath::Rotation {
        let q = quat_normalize([out[0], out[1], out[2], out[3]]);
        out.copy_from_slice(&q);
      }
    }
  }
}

impl Spatial {
  /// Register a baked clip. Channel lengths are validated here (the one
  /// indexing safety boundary); key-time ordering is the baker's contract.
  pub fn create_clip(&mut self, duration: f64, channels: Vec<ClipChannel>) -> Result<ClipId, String> {
    for (i, c) in channels.iter().enumerate() {
      let keys = c.times.len();
      if keys == 0 {
        return Err(format!("clip channel {i} has no keys"));
      }
      let stride = if c.interpolation == ChannelInterpolation::Cubic { c.path.elements() * 3 } else { c.path.elements() };
      if c.values.len() != keys * stride {
        return Err(format!(
          "clip channel {i} has {} keys but {} values ({} per key expected)",
          keys,
          c.values.len(),
          stride
        ));
      }
    }
    let id = self.players.next_clip;
    self.players.next_clip += 1;
    self.players.clips.insert(id, Clip { duration, channels });
    Ok(id)
  }

  /// Free a clip. Players still on it drop at their next advance with a
  /// Dropped event.
  pub fn destroy_clip(&mut self, id: ClipId) -> Result<(), String> {
    self.players.clips.remove(&id).map(|_| ()).ok_or_else(|| format!("clip {id} not found"))
  }

  /// Start a player: `targets[slot]` is the node each channel's
  /// `target_slot` animates. Every target must resolve NOW (animation
  /// binds live arena nodes; a model animates while in a scene) - later
  /// deaths drop the player with a Dropped event instead.
  pub fn create_player(
    &mut self,
    clip: ClipId,
    targets: Vec<NodeId>,
    speed: f32,
    looped: bool,
    weight: f32,
    fade: f32,
  ) -> Result<PlayerId, String> {
    let clip_ref = self.players.clips.get(&clip).ok_or_else(|| format!("clip {clip} not found"))?;
    for (i, c) in clip_ref.channels.iter().enumerate() {
      if c.target_slot as usize >= targets.len() {
        return Err(format!(
          "clip channel {i} targets slot {}, only {} targets given",
          c.target_slot,
          targets.len()
        ));
      }
    }
    let channels = clip_ref.channels.len();
    for &t in &targets {
      self.resolve(t)?;
    }
    let id = self.players.next_player;
    self.players.next_player += 1;
    self.players.players.push(Player {
      id,
      clip,
      targets,
      time: 0.0,
      speed,
      weight,
      fade,
      looped,
      finished: false,
      cursors: vec![0; channels],
    });
    Ok(id)
  }

  /// Write the given fields of a player (the crossfade channel). Unknown
  /// ids err - a Dropped player is gone.
  pub fn set_player(&mut self, id: PlayerId, update: PlayerUpdate) -> Result<(), String> {
    let p = self
      .players
      .players
      .iter_mut()
      .find(|p| p.id == id)
      .ok_or_else(|| format!("player {id} not found"))?;
    if let Some(w) = update.weight {
      p.weight = w.clamp(0.0, 1.0);
    }
    if let Some(f) = update.fade {
      p.fade = f;
    }
    if let Some(s) = update.speed {
      p.speed = s;
    }
    if let Some(t) = update.time {
      p.time = t;
      p.finished = false;
    }
    Ok(())
  }

  /// Remove a player at once (no event; stop-with-fade is a `set_player`
  /// fade write instead). A missing id is fine - it may have dropped.
  pub fn destroy_player(&mut self, id: PlayerId) {
    self.players.players.retain(|p| p.id != id);
  }

  /// The node's current local TRS (what the players last wrote, or any
  /// later snap): position, quaternion, scale. The pose read for
  /// root-motion strips and skeleton copies.
  pub fn transform_of(&self, id: NodeId) -> Result<([f32; 3], [f32; 4], [f32; 3]), String> {
    let n = &self.nodes[self.resolve(id)? as usize];
    Ok((n.position, n.rotation, n.scale))
  }

  /// Advance every player to the stamped clock (`set_transition_now` - the
  /// runtime stamps before any frame work) and write the blended poses
  /// through the snap path. Called once per frame BEFORE the frame's JS;
  /// the embedder drains `take_clip_events` right after and keeps
  /// requesting frames while `active`.
  pub fn advance_players(&mut self) -> PlayersTick {
    let now = self.transitions.now_ms;
    let dt = ((now - self.players.last_ms).max(0.0)) / 1000.0;
    self.players.last_ms = now;
    if self.players.players.is_empty() {
      return PlayersTick::default();
    }

    // Clocks, fades, lifecycle. A player whose clip or any target died
    // drops here (Dropped); one fading past 0 drops too; a non-looping
    // one reaching its end reports Finished once and holds.
    let mut i = 0;
    while i < self.players.players.len() {
      let set = &mut self.players;
      let alive = set.clips.contains_key(&set.players[i].clip)
        && set.players[i].targets.iter().all(|&t| {
          let idx = super::index(t);
          self.nodes.get(idx).is_some_and(|n| n.alive && n.generation == super::generation(t))
        });
      if !alive {
        let id = set.players[i].id;
        set.events.push(ClipEvent::Dropped(id));
        set.players.remove(i);
        continue;
      }
      let duration = set.clips[&set.players[i].clip].duration;
      let p = &mut set.players[i];
      if p.fade != 0.0 {
        p.weight += p.fade * dt as f32;
        if p.weight >= 1.0 {
          p.weight = 1.0;
          p.fade = 0.0;
        } else if p.weight <= 0.0 && p.fade < 0.0 {
          // Fading OUT past zero removes; a fade-IN sitting at zero (just
          // created, or a dt-0 advance under a frozen clock) stays.
          let id = p.id;
          set.events.push(ClipEvent::Dropped(id));
          set.players.remove(i);
          continue;
        }
      }
      p.time += dt * p.speed as f64;
      if duration <= 0.0 {
        p.time = 0.0;
      } else if p.looped {
        p.time = ((p.time % duration) + duration) % duration;
      } else if p.time >= duration {
        p.time = duration;
        if !p.finished {
          p.finished = true;
          let id = p.id;
          set.events.push(ClipEvent::Finished(id));
        }
      } else if p.time < 0.0 {
        p.time = 0.0;
      }
      i += 1;
    }

    // Blend: per (node, path), the weighted average over the players that
    // animate it - the mixer's incremental accumulation, verbatim.
    let mut slots = std::mem::take(&mut self.players.slots);
    slots.clear();
    let mut out = [0.0f32; 4];
    for pi in 0..self.players.players.len() {
      let set = &mut self.players;
      let clip = &set.clips[&set.players[pi].clip];
      let (time, weight) = (set.players[pi].time as f32, set.players[pi].weight);
      for (ci, channel) in clip.channels.iter().enumerate() {
        let target = set.players[pi].targets[channel.target_slot as usize];
        sample(channel, time, &mut set.players[pi].cursors[ci], &mut out);
        let elements = channel.path.elements();
        let key = (target, channel.path as u8);
        match slots.get_mut(&key) {
          None => {
            let mut value = [0.0f32; 4];
            value[..elements].copy_from_slice(&out[..elements]);
            slots.insert(key, BlendSlot { sum: weight, value });
          }
          Some(slot) => {
            let total = slot.sum + weight;
            let share = if total > 0.0 { weight / total } else { 0.0 };
            slot.sum = total;
            if channel.path == ChannelPath::Rotation {
              slot.value = slerp(slot.value, out, share);
            } else {
              for e in 0..elements {
                slot.value[e] += (out[e] - slot.value[e]) * share;
              }
            }
          }
        }
      }
    }

    // Write the blended poses through the snap path; unchanged values
    // write nothing, so a frozen clock settles to zero work.
    let mut wrote = false;
    for (&(node, path), slot) in slots.iter() {
      let Ok(idx) = self.resolve(node) else { continue };
      let n = &mut self.nodes[idx as usize];
      let changed = match path {
        0 => {
          let v = [slot.value[0], slot.value[1], slot.value[2]];
          if n.position != v {
            n.position = v;
            true
          } else {
            false
          }
        }
        1 => {
          if n.rotation != slot.value {
            n.rotation = slot.value;
            true
          } else {
            false
          }
        }
        _ => {
          let v = [slot.value[0], slot.value[1], slot.value[2]];
          if n.scale != v {
            n.scale = v;
            true
          } else {
            false
          }
        }
      };
      if changed {
        self.nodes[idx as usize].local_dirty = true;
        self.enqueue(idx);
        wrote = true;
      }
    }
    self.players.slots = slots;

    // Active = a player that can still progress: unfinished, or fading.
    let active = self.players.players.iter().any(|p| !p.finished || p.fade != 0.0);
    PlayersTick { active, wrote }
  }

  /// The Finished/Dropped events since the last drain, in order.
  pub fn take_clip_events(&mut self) -> Vec<ClipEvent> {
    std::mem::take(&mut self.players.events)
  }
}
