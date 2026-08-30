use crate::color::{color_to_oklab, oklab_to_color};
use crate::impellers::Color;
use crate::motion::spring_step;
use crate::rendertree::{Damage, Element, ElementKind, OriginCoord};

pub use crate::motion::{Curve, TransitionSpec};

// Native transitions (okf/done/native-transitions.md): a `transition`
// declaration on an element makes numeric property writes animate on the
// Rust side. JS writes only targets; the tree owns time. This module holds
// the element-side pieces: the per-element config, the per-property track
// state, and the property mapping. The curve/spring math and the spec
// vocabulary live in `crate::motion` (shared with the spatial arena's node
// transitions); the tree-level plumbing (starting, advancing and settling
// tracks against nodes) lives in tree.rs, where the node map and damage
// application are.

/// A property a transition can animate: the numeric-scalar set. The JSX name
/// to variant mapping lives in the plugin layer (flux); the tree side works
/// in these ids only. Which kinds carry which variant is answered by
/// `Element::anim_value` below - a variant a kind does not have simply reads
/// as None there, and the write falls back to the normal (snapping) path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimProp {
  // Detached-only geometry (rect/oval/text/texture; path has x/y only), and
  // on a view the transform translation.
  X,
  Y,
  W,
  H,
  // Line endpoints (detached-only).
  X1,
  Y1,
  X2,
  Y2,
  // View-only.
  ScrollX,
  ScrollY,
  Opacity,
  // Transform origin, pixel form only (a fraction origin snaps).
  OriginX,
  OriginY,
  Perspective,
  // Clip corner radius, single-number form only (like Radius).
  ClipRadius,
  // Texture source crop (both forms; the crop is paint state, not geometry).
  SrcX,
  SrcY,
  SrcW,
  SrcH,
  // Line dash pattern.
  OnLength,
  OffLength,
  DashOffset,
  Rotate,
  RotateX,
  RotateY,
  Scale,
  ScaleX,
  ScaleY,
  // Shared paint state (kinds with a PaintState).
  StrokeWidth,
  // Solid paint color, interpolated in oklab. A gradient never animates.
  Color,
  // Rect corner radius, single-number form only.
  Radius,
}

/// A value an animatable property carries: the scalar set, plus solid
/// colors. Colors interpolate in oklab (with alpha as its own linear lane),
/// so a red-to-blue transition passes through neither gray nor purple mud.
#[derive(Clone, Copy, Debug)]
pub enum AnimValue {
  Scalar(f32),
  Color(Color),
}

/// One declared transition: the motion spec plus the conveniences that ride
/// with it. `delay_ms` holds each write for that long (animation-clock time)
/// before it applies; `from` seeds a mount-time enter animation - at the
/// node's first attach the property snaps to `from` and animates to the
/// value it mounted with; `exit` seeds the removal animation - a removed
/// node stays in the tree, animates the property to `exit`, and is freed
/// when its exit tracks settle (see tree.rs `detach_node`). `from` and
/// `exit` are meaningful on per-property entries only (the decoder rejects
/// them under `all`).
#[derive(Clone, Copy, Debug)]
pub struct TransitionEntry {
  pub spec: TransitionSpec,
  pub delay_ms: f32,
  pub from: Option<AnimValue>,
  pub exit: Option<AnimValue>,
}

impl From<TransitionSpec> for TransitionEntry {
  fn from(spec: TransitionSpec) -> Self {
    TransitionEntry { spec, delay_ms: 0.0, from: None, exit: None }
  }
}

/// The transition declaration an element carries: per-property entries plus
/// an `all` catch-all. Applies to writes from the moment it is set; it does
/// not retroactively affect running tracks.
///
/// `stagger_ms` makes the element a stagger group: every descendant enter
/// (`from`) or exit that begins in the same frame under this element gets
/// `index * stagger_ms` of extra delay, indexed in occurrence order (enters
/// and exits count separately). The nearest declaring ancestor wins; nested
/// groups never compound. It orchestrates descendants only - the element's
/// own lifecycle is staggered by ITS ancestors, and ordinary property
/// writes never stagger.
#[derive(Clone, Debug, Default)]
pub struct TransitionConfig {
  pub props: Vec<(AnimProp, TransitionEntry)>,
  pub all: Option<TransitionEntry>,
  pub stagger_ms: Option<f32>,
}

impl TransitionConfig {
  pub fn entry_for(&self, prop: AnimProp) -> Option<TransitionEntry> {
    self.props.iter().find(|(p, _)| *p == prop).map(|(_, e)| *e).or(self.all)
  }
}

/// A write held by `delay`: it applies (starts or retargets a track) when
/// the animation clock reaches `at_ms`, exactly as if JS had written it
/// then. Until then the element keeps whatever motion or rest it had - a
/// running track toward an older target keeps going and may settle (and
/// fire its end event) naturally. A newer write for the pair replaces the
/// hold; a snap write drops it.
#[derive(Clone, Copy, Debug)]
pub struct PendingWrite {
  pub node: u64,
  pub prop: AnimProp,
  pub to: AnimValue,
  pub spec: TransitionSpec,
  pub at_ms: f64,
}

// Track values are lane vectors: scalars use one lane, colors four (oklab
// L/a/b plus alpha). Tween and spring math run per lane; a color spring is
// four independent oscillators sharing one spec.
pub type Lanes = [f32; 4];

fn to_lanes(v: AnimValue) -> (Lanes, bool) {
  match v {
    AnimValue::Scalar(s) => ([s, 0.0, 0.0, 0.0], false),
    AnimValue::Color(c) => (color_to_oklab(c), true),
  }
}

fn from_lanes(lanes: Lanes, color: bool) -> AnimValue {
  if color {
    AnimValue::Color(oklab_to_color(lanes))
  } else {
    AnimValue::Scalar(lanes[0])
  }
}

/// Interpolation state of one running track.
#[derive(Clone, Copy, Debug)]
pub enum TrackState {
  Tween {
    from: Lanes,
    start_ms: f64,
  },
  /// Position and velocity (per second), integrated each frame.
  Spring {
    pos: Lanes,
    vel: Lanes,
  },
}

/// One running animation: a (node, prop) pair moving toward `to`.
#[derive(Debug)]
pub struct Track {
  pub node: u64,
  pub prop: AnimProp,
  pub spec: TransitionSpec,
  pub state: TrackState,
  to: Lanes,
  // Lanes encode a color (write back as Color) rather than a scalar.
  color: bool,
  // Settle threshold, scaled to the animated distance so pixels and
  // unit-scale values (opacity, oklab) both settle promptly.
  eps: f32,
}

fn eps_for(from: Lanes, to: Lanes) -> f32 {
  let mut d = 0.0f32;
  for i in 0..4 {
    d = d.max((to[i] - from[i]).abs());
  }
  (d.max(1.0) * 1e-3).max(1e-3)
}

impl Track {
  pub fn target(&self) -> AnimValue {
    from_lanes(self.to, self.color)
  }

  /// Advance to `now_ms` (`dt_ms` since the previous advance, for the
  /// spring). Returns the value to write and whether the track settled; a
  /// settled track reports the target exactly.
  pub fn advance(&mut self, now_ms: f64, dt_ms: f64) -> (AnimValue, bool) {
    let color = self.color;
    let (lanes, settled) = self.advance_lanes(now_ms, dt_ms);
    (from_lanes(lanes, color), settled)
  }

  fn advance_lanes(&mut self, now_ms: f64, dt_ms: f64) -> (Lanes, bool) {
    match (&mut self.state, self.spec) {
      (TrackState::Tween { from, start_ms }, TransitionSpec::Tween { duration_ms, curve }) => {
        let p = ((now_ms - *start_ms) / duration_ms as f64).clamp(0.0, 1.0) as f32;
        if p >= 1.0 {
          return (self.to, true);
        }
        let e = curve.eval(p);
        let mut out = *from;
        for i in 0..4 {
          out[i] += (self.to[i] - out[i]) * e;
        }
        (out, false)
      }
      (TrackState::Spring { pos, vel }, TransitionSpec::Spring { omega, zeta }) => {
        let dt = (dt_ms / 1000.0) as f32;
        let mut settled = true;
        for i in 0..4 {
          let (x, v) = spring_step(pos[i] - self.to[i], vel[i], omega, zeta, dt);
          pos[i] = self.to[i] + x;
          vel[i] = v;
          if x.abs() >= self.eps || v.abs() >= self.eps * omega {
            settled = false;
          }
        }
        if settled {
          return (self.to, true);
        }
        (*pos, false)
      }
      // Spec kind and state kind are paired at creation and on retarget;
      // a mismatch cannot arise, but settle instantly rather than panic.
      _ => (self.to, true),
    }
  }
}

/// Tree-level transition state: the running tracks and the animation clock,
/// stamped once per frame from the app timeline (the paced clock) before the
/// JS frame work runs, so writes and the advance agree on time.
#[derive(Default)]
pub struct Transitions {
  pub now_ms: f64,
  // Time of the previous advance, the spring dt reference. Reset when the
  // track list becomes non-empty so an idle gap never enters a spring.
  last_ms: f64,
  tracks: Vec<Track>,
  // Delayed writes waiting for their activation time (tree.rs drains the
  // due ones at the top of each advance).
  pending: Vec<PendingWrite>,
  // (node, prop) pairs whose track settled, awaiting the embedder's drain
  // (the onTransitionEnd dispatch). Cancelled tracks never land here.
  pub settled: Vec<(u64, AnimProp)>,
  // Per-frame stagger counters, keyed by (group ancestor, is_exit): how many
  // descendant enters/exits the group has seen this frame. Cleared at every
  // clock stamp (tree.rs set_transition_now), so a batch mounted in one tick
  // cascades and later frames start their own count at zero.
  stagger_counts: std::collections::HashMap<(u64, bool), u32>,
}

impl Transitions {
  pub fn is_empty(&self) -> bool {
    self.tracks.is_empty() && self.pending.is_empty()
  }

  /// The next stagger index for a lifecycle event under `group` this frame
  /// (post-incremented). Enters and exits count separately, so a swap that
  /// removes and mounts in one tick runs two clean cascades.
  pub fn stagger_index(&mut self, group: u64, exit: bool) -> u32 {
    let count = self.stagger_counts.entry((group, exit)).or_insert(0);
    let index = *count;
    *count += 1;
    index
  }

  pub fn reset_stagger(&mut self) {
    self.stagger_counts.clear();
  }

  /// Hold a delayed write until its activation time. One hold per
  /// (node, prop): a newer write replaces it, delay restarted.
  pub fn schedule(&mut self, write: PendingWrite) {
    self.unschedule(write.node, write.prop);
    self.pending.push(write);
  }

  pub fn unschedule(&mut self, node: u64, prop: AnimProp) {
    self.pending.retain(|w| !(w.node == node && w.prop == prop));
  }

  /// Drain the pending writes whose activation time has arrived.
  pub fn take_due(&mut self, now_ms: f64) -> Vec<PendingWrite> {
    let mut due = Vec::new();
    self.pending.retain(|w| {
      if w.at_ms <= now_ms {
        due.push(*w);
        false
      } else {
        true
      }
    });
    due
  }

  /// Start or retarget the track for (node, prop). `current` is the
  /// property's present value (the from-value for a fresh or restarted
  /// tween); a running spring keeps its position and velocity and only moves
  /// its equilibrium. `current` and `to` must be the same AnimValue kind
  /// (the caller guarantees it by reading `current` for the same property).
  /// Returns whether a track now runs for the pair - false means the value
  /// already sits on the target and there was nothing to animate.
  pub fn retarget(
    &mut self,
    node: u64,
    prop: AnimProp,
    current: AnimValue,
    to: AnimValue,
    spec: TransitionSpec,
  ) -> bool {
    let now = self.now_ms;
    let (cur, color) = to_lanes(current);
    let (to, _) = to_lanes(to);
    if let Some(t) = self.tracks.iter_mut().find(|t| t.node == node && t.prop == prop) {
      t.to = to;
      t.eps = eps_for(cur, to);
      let keep_spring_state = matches!((&t.state, spec), (TrackState::Spring { .. }, TransitionSpec::Spring { .. }));
      t.spec = spec;
      if !keep_spring_state {
        t.state = match spec {
          TransitionSpec::Tween { .. } => TrackState::Tween { from: cur, start_ms: now },
          TransitionSpec::Spring { .. } => TrackState::Spring { pos: cur, vel: [0.0; 4] },
        };
      }
      return true;
    }
    if to == cur {
      return false;
    }
    if self.tracks.is_empty() {
      self.last_ms = now;
    }
    let state = match spec {
      TransitionSpec::Tween { .. } => TrackState::Tween { from: cur, start_ms: now },
      TransitionSpec::Spring { .. } => TrackState::Spring { pos: cur, vel: [0.0; 4] },
    };
    self.tracks.push(Track { node, prop, spec, state, to, color, eps: eps_for(cur, to) });
    true
  }

  /// Drop the track and any pending write for (node, prop): a non-animated
  /// write to the property took over (null reset, non-numeric value).
  pub fn cancel(&mut self, node: u64, prop: AnimProp) {
    self.tracks.retain(|t| !(t.node == node && t.prop == prop));
    self.unschedule(node, prop);
  }

  /// Drop every track and pending write of a node (it is being freed, or an
  /// exit was abandoned by a re-insert).
  pub fn cancel_props(&mut self, node: u64, props: &[AnimProp]) {
    self.tracks.retain(|t| !(t.node == node && props.contains(&t.prop)));
    self.pending.retain(|w| !(w.node == node && props.contains(&w.prop)));
  }

  pub fn cancel_node(&mut self, node: u64) {
    self.tracks.retain(|t| t.node != node);
    self.pending.retain(|w| w.node != node);
  }

  /// Whether any track or pending write runs for the node on one of `props`
  /// (the exiting-node liveness check).
  pub fn any_running(&self, node: u64, props: &[AnimProp]) -> bool {
    self.tracks.iter().any(|t| t.node == node && props.contains(&t.prop))
      || self.pending.iter().any(|w| w.node == node && props.contains(&w.prop))
  }

  /// Take the track list for an advance pass (tree.rs), leaving the clock in
  /// place. Returns the tracks and the dt since the previous advance.
  pub fn begin_advance(&mut self) -> (Vec<Track>, f64) {
    let dt = (self.now_ms - self.last_ms).max(0.0);
    self.last_ms = self.now_ms;
    (std::mem::take(&mut self.tracks), dt)
  }

  pub fn end_advance(&mut self, mut tracks: Vec<Track>) {
    // Writes from inside the advance (none today) would have pushed new
    // tracks; keep both.
    tracks.append(&mut self.tracks);
    self.tracks = tracks;
  }
}

impl Element {
  /// Current value of an animatable property on this element, `None` when
  /// the kind does not carry it (or carries it only in detached form and
  /// this element has a layout box, mirroring the detached-only rule of the
  /// property path). A `None` makes the write fall back to the normal
  /// (snapping) property path, which raises the proper error.
  pub fn anim_value(&self, prop: AnimProp) -> Option<AnimValue> {
    use AnimProp::*;
    let detached = !self.has_layout();
    if prop == Color {
      // A gradient (or a gradient taking over mid-track) never animates.
      let paint = self.kind.paint()?;
      if paint.gradient.is_some() {
        return None;
      }
      return Some(AnimValue::Color(paint.color));
    }
    let scalar = match (&self.kind, prop) {
      (ElementKind::View(v), X) => Some(v.translate.map(|t| t.x).unwrap_or(0.0)),
      (ElementKind::View(v), Y) => Some(v.translate.map(|t| t.y).unwrap_or(0.0)),
      (ElementKind::View(v), ScrollX) => Some(v.scroll.map(|s| s.x).unwrap_or(0.0)),
      (ElementKind::View(v), ScrollY) => Some(v.scroll.map(|s| s.y).unwrap_or(0.0)),
      (ElementKind::View(v), Opacity) => Some(v.opacity.unwrap_or(1.0)),
      // An unset origin is the box center, a fraction: neither has a pixel
      // value to animate from.
      (ElementKind::View(v), OriginX) => match v.origin_x {
        Some(OriginCoord::Px(px)) => Some(px),
        _ => None,
      },
      (ElementKind::View(v), OriginY) => match v.origin_y {
        Some(OriginCoord::Px(px)) => Some(px),
        _ => None,
      },
      (ElementKind::View(v), Perspective) => v.perspective,
      (ElementKind::View(v), ClipRadius) => match v.clip_radius {
        None => Some(0.0),
        Some([a, b, c, d]) if a == b && b == c && c == d => Some(a),
        Some(_) => None,
      },
      (ElementKind::Texture(t), SrcX) => Some(t.src_x.unwrap_or(0.0)),
      (ElementKind::Texture(t), SrcY) => Some(t.src_y.unwrap_or(0.0)),
      // Crop size defaults to the texture's own, unknowable here: animate
      // only from an explicit value.
      (ElementKind::Texture(t), SrcW) => t.src_w,
      (ElementKind::Texture(t), SrcH) => t.src_h,
      (ElementKind::Line(l), OnLength) => Some(l.on_length.unwrap_or(0.0)),
      (ElementKind::Line(l), OffLength) => Some(l.off_length.unwrap_or(0.0)),
      (ElementKind::Line(l), DashOffset) => Some(l.dash_offset.unwrap_or(0.0)),
      (ElementKind::Path(p), OnLength) => Some(p.on_length.unwrap_or(0.0)),
      (ElementKind::Path(p), OffLength) => Some(p.off_length.unwrap_or(0.0)),
      (ElementKind::Path(p), DashOffset) => Some(p.dash_offset.unwrap_or(0.0)),
      (ElementKind::Rectangle(r), OnLength) => Some(r.on_length.unwrap_or(0.0)),
      (ElementKind::Rectangle(r), OffLength) => Some(r.off_length.unwrap_or(0.0)),
      (ElementKind::Rectangle(r), DashOffset) => Some(r.dash_offset.unwrap_or(0.0)),
      (ElementKind::Oval(o), OnLength) => Some(o.on_length.unwrap_or(0.0)),
      (ElementKind::Oval(o), OffLength) => Some(o.off_length.unwrap_or(0.0)),
      (ElementKind::Oval(o), DashOffset) => Some(o.dash_offset.unwrap_or(0.0)),
      (ElementKind::View(v), Rotate) => Some(v.rotate.unwrap_or(0.0)),
      (ElementKind::View(v), RotateX) => Some(v.rotate_x.unwrap_or(0.0)),
      (ElementKind::View(v), RotateY) => Some(v.rotate_y.unwrap_or(0.0)),
      (ElementKind::View(v), Scale) | (ElementKind::View(v), ScaleX) => Some(v.scale_x.unwrap_or(1.0)),
      (ElementKind::View(v), ScaleY) => Some(v.scale_y.unwrap_or(1.0)),

      (ElementKind::Rectangle(r), X) if detached => Some(r.x.unwrap_or(0.0)),
      (ElementKind::Rectangle(r), Y) if detached => Some(r.y.unwrap_or(0.0)),
      // W/H default to the inherited box, unknowable here: animate only from
      // an explicit value.
      (ElementKind::Rectangle(r), W) if detached => r.w,
      (ElementKind::Rectangle(r), H) if detached => r.h,
      // Radius animates in its single-number form only: from an unset radius
      // (0) or four equal corners.
      (ElementKind::Rectangle(r), Radius) => match r.radius {
        None => Some(0.0),
        Some([a, b, c, d]) if a == b && b == c && c == d => Some(a),
        Some(_) => None,
      },

      (ElementKind::Oval(o), X) if detached => Some(o.x.unwrap_or(0.0)),
      (ElementKind::Oval(o), Y) if detached => Some(o.y.unwrap_or(0.0)),
      (ElementKind::Oval(o), W) if detached => o.w,
      (ElementKind::Oval(o), H) if detached => o.h,

      (ElementKind::Text(t), X) if detached => Some(t.x.unwrap_or(0.0)),
      (ElementKind::Text(t), Y) if detached => Some(t.y.unwrap_or(0.0)),

      (ElementKind::Texture(t), X) if detached => Some(t.x.unwrap_or(0.0)),
      (ElementKind::Texture(t), Y) if detached => Some(t.y.unwrap_or(0.0)),
      (ElementKind::Texture(t), W) if detached => t.w,
      (ElementKind::Texture(t), H) if detached => t.h,

      (ElementKind::Path(p), X) if detached => Some(p.x.unwrap_or(0.0)),
      (ElementKind::Path(p), Y) if detached => Some(p.y.unwrap_or(0.0)),

      (ElementKind::Line(l), X1) if detached => Some(l.x1.unwrap_or(0.0)),
      (ElementKind::Line(l), Y1) if detached => Some(l.y1.unwrap_or(0.0)),
      // x2/y2 default to the inherited box size, unknowable here: animate
      // only from an explicit value.
      (ElementKind::Line(l), X2) if detached => l.x2,
      (ElementKind::Line(l), Y2) if detached => l.y2,

      (_, StrokeWidth) => self.kind.paint().map(|p| p.stroke_width),

      _ => None,
    };
    scalar.map(AnimValue::Scalar)
  }

  /// Write an animatable property through its typed setter, returning the
  /// setter's damage. Must accept exactly the (kind, prop) pairs
  /// `anim_value` answers; anything else is a no-op (`Damage::None`).
  pub fn set_anim_value(&mut self, prop: AnimProp, value: AnimValue) -> Damage {
    use AnimProp::*;
    let detached = !self.has_layout();
    if let AnimValue::Color(c) = value {
      return match (prop, self.kind.paint_mut()) {
        (Color, Some(p)) => p.set_color(Some(c)),
        _ => Damage::None,
      };
    }
    let AnimValue::Scalar(v) = value else {
      return Damage::None;
    };
    match (&mut self.kind, prop) {
      (ElementKind::View(view), X) => view.set_x(Some(v)),
      (ElementKind::View(view), Y) => view.set_y(Some(v)),
      (ElementKind::View(view), ScrollX) => view.set_scroll_x(Some(v)),
      (ElementKind::View(view), ScrollY) => view.set_scroll_y(Some(v)),
      (ElementKind::View(view), Opacity) => view.set_opacity(Some(v)),
      (ElementKind::View(view), OriginX) => view.set_origin_x(Some(OriginCoord::Px(v))),
      (ElementKind::View(view), OriginY) => view.set_origin_y(Some(OriginCoord::Px(v))),
      (ElementKind::View(view), Perspective) => view.set_perspective(Some(v)),
      (ElementKind::View(view), ClipRadius) => view.set_clip_radius(Some([v.max(0.0); 4])),
      (ElementKind::Texture(t), SrcX) => t.set_src_x(Some(v)),
      (ElementKind::Texture(t), SrcY) => t.set_src_y(Some(v)),
      (ElementKind::Texture(t), SrcW) => t.set_src_w(Some(v)),
      (ElementKind::Texture(t), SrcH) => t.set_src_h(Some(v)),
      (ElementKind::Line(l), OnLength) => l.set_on_length(Some(v)),
      (ElementKind::Line(l), OffLength) => l.set_off_length(Some(v)),
      (ElementKind::Line(l), DashOffset) => l.set_dash_offset(Some(v)),
      (ElementKind::Path(p), OnLength) => p.set_on_length(Some(v)),
      (ElementKind::Path(p), OffLength) => p.set_off_length(Some(v)),
      (ElementKind::Path(p), DashOffset) => p.set_dash_offset(Some(v)),
      (ElementKind::Rectangle(r), OnLength) => r.set_on_length(Some(v)),
      (ElementKind::Rectangle(r), OffLength) => r.set_off_length(Some(v)),
      (ElementKind::Rectangle(r), DashOffset) => r.set_dash_offset(Some(v)),
      (ElementKind::Oval(o), OnLength) => o.set_on_length(Some(v)),
      (ElementKind::Oval(o), OffLength) => o.set_off_length(Some(v)),
      (ElementKind::Oval(o), DashOffset) => o.set_dash_offset(Some(v)),
      (ElementKind::View(view), Rotate) => view.set_rotate(Some(v)),
      (ElementKind::View(view), RotateX) => view.set_rotate_x(Some(v)),
      (ElementKind::View(view), RotateY) => view.set_rotate_y(Some(v)),
      (ElementKind::View(view), Scale) => {
        view.set_scale_x(Some(v));
        view.set_scale_y(Some(v))
      }
      (ElementKind::View(view), ScaleX) => view.set_scale_x(Some(v)),
      (ElementKind::View(view), ScaleY) => view.set_scale_y(Some(v)),

      (ElementKind::Rectangle(r), X) if detached => r.set_x(Some(v)),
      (ElementKind::Rectangle(r), Y) if detached => r.set_y(Some(v)),
      (ElementKind::Rectangle(r), W) if detached => r.set_w(Some(v)),
      (ElementKind::Rectangle(r), H) if detached => r.set_h(Some(v)),
      (ElementKind::Rectangle(r), Radius) => r.set_radius(Some([v.max(0.0); 4])),

      (ElementKind::Oval(o), X) if detached => o.set_x(Some(v)),
      (ElementKind::Oval(o), Y) if detached => o.set_y(Some(v)),
      (ElementKind::Oval(o), W) if detached => o.set_w(Some(v)),
      (ElementKind::Oval(o), H) if detached => o.set_h(Some(v)),

      (ElementKind::Text(t), X) if detached => t.set_x(Some(v)),
      (ElementKind::Text(t), Y) if detached => t.set_y(Some(v)),

      (ElementKind::Texture(t), X) if detached => t.set_x(Some(v)),
      (ElementKind::Texture(t), Y) if detached => t.set_y(Some(v)),
      (ElementKind::Texture(t), W) if detached => t.set_w(Some(v)),
      (ElementKind::Texture(t), H) if detached => t.set_h(Some(v)),

      (ElementKind::Path(p), X) if detached => p.set_x(Some(v)),
      (ElementKind::Path(p), Y) if detached => p.set_y(Some(v)),

      (ElementKind::Line(l), X1) if detached => l.set_x1(Some(v)),
      (ElementKind::Line(l), Y1) if detached => l.set_y1(Some(v)),
      (ElementKind::Line(l), X2) if detached => l.set_x2(Some(v)),
      (ElementKind::Line(l), Y2) if detached => l.set_y2(Some(v)),

      (kind, StrokeWidth) => match kind.paint_mut() {
        Some(p) => p.set_stroke_width(Some(v.max(0.0))),
        None => Damage::None,
      },

      _ => Damage::None,
    }
  }
}

#[cfg(test)]
pub(crate) fn test_oklab_roundtrip(c: Color) -> Color {
  oklab_to_color(color_to_oklab(c))
}
