use crate::rendertree::{Damage, Element, ElementKind};

// Native transitions (okf/backlog/native-transitions.md): a `transition`
// declaration on an element makes numeric property writes animate on the
// Rust side. JS writes only targets; the tree owns time. This module holds
// the engine-free pieces: the per-element config, the per-property track
// state, and the tween/spring math. The tree-level plumbing (starting,
// advancing and settling tracks against nodes) lives in tree.rs, where the
// node map and damage application are.

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
  Opacity,
  Rotate,
  RotateX,
  RotateY,
  Scale,
  ScaleX,
  ScaleY,
  // Shared paint state (kinds with a PaintState).
  StrokeWidth,
  // Rect corner radius, single-number form only.
  Radius,
}

/// A tween's easing curve. Named CSS curves are decoded to their bezier
/// control points in the plugin layer; `Linear` is the identity.
#[derive(Clone, Copy, Debug)]
pub enum Curve {
  Linear,
  /// cubic-bezier(x1, y1, x2, y2), the CSS timing function: endpoints fixed
  /// at (0,0) and (1,1), progress in on x, eased progress out on y.
  Bezier(f32, f32, f32, f32),
}

impl Curve {
  /// Eased progress for linear progress `p` in [0, 1]. The bezier is solved
  /// for the parameter t with x(t) = p (Newton with a bisection fallback,
  /// the standard UnitBezier scheme), then evaluated on y.
  pub fn eval(&self, p: f32) -> f32 {
    match *self {
      Curve::Linear => p,
      Curve::Bezier(x1, y1, x2, y2) => {
        // Polynomial coefficients from the control values: for control
        // points c1, c2 the curve is ((a*t + b)*t + c)*t with
        // c = 3*c1, b = 3*(c2 - c1) - c, a = 1 - c - b.
        let coeff = |c1: f32, c2: f32| {
          let c = 3.0 * c1;
          let b = 3.0 * (c2 - c1) - c;
          let a = 1.0 - c - b;
          (a, b, c)
        };
        let (ax, bx, cx) = coeff(x1, x2);
        let (ay, by, cy) = coeff(y1, y2);
        let sample = |a: f32, b: f32, c: f32, t: f32| ((a * t + b) * t + c) * t;

        // Newton-Raphson on x(t) - p.
        let mut t = p;
        let mut solved = false;
        for _ in 0..8 {
          let x = sample(ax, bx, cx, t) - p;
          if x.abs() < 1e-5 {
            solved = true;
            break;
          }
          let dx = (3.0 * ax * t + 2.0 * bx) * t + cx;
          if dx.abs() < 1e-6 {
            break;
          }
          t -= x / dx;
        }
        if !solved {
          // Bisection fallback: x(t) is monotone in t for valid curves.
          let (mut lo, mut hi) = (0.0f32, 1.0f32);
          t = p;
          for _ in 0..20 {
            let x = sample(ax, bx, cx, t);
            if (x - p).abs() < 1e-5 {
              break;
            }
            if x < p {
              lo = t;
            } else {
              hi = t;
            }
            t = (lo + hi) * 0.5;
          }
        }
        sample(ay, by, cy, t.clamp(0.0, 1.0))
      }
    }
  }
}

/// How one property animates toward a new target. Both forms take their
/// duration in milliseconds, matching the platform's time vocabulary
/// (timers, performance.now).
#[derive(Clone, Copy, Debug)]
pub enum TransitionSpec {
  Tween {
    duration_ms: f32,
    curve: Curve,
  },
  /// A damped harmonic oscillator in derived form: `omega` (rad/s) and the
  /// damping ratio `zeta`. Built from the perceptual pair via `spring` -
  /// physics parameters (stiffness/damping/mass) are deliberately not part
  /// of the API surface.
  Spring {
    omega: f32,
    zeta: f32,
  },
}

impl TransitionSpec {
  /// A spring from the perceptual pair: `duration_ms` the perceptual
  /// settling time, `bounce` in (-1, 1] with 0 critically damped. The
  /// mapping (SwiftUI's model, mass 1): omega = 2*pi/duration_s,
  /// zeta = 1 - bounce for bounce >= 0, and for bounce < 0 the overdamped
  /// zeta = 1/(1 + bounce). Inputs are assumed validated (positive
  /// duration, bounce > -1); decode-time validation is the plugin's job.
  pub fn spring(duration_ms: f32, bounce: f32) -> Self {
    let omega = 2.0 * std::f32::consts::PI / (duration_ms / 1000.0);
    let zeta = if bounce >= 0.0 { 1.0 - bounce } else { 1.0 / (1.0 + bounce) };
    TransitionSpec::Spring { omega, zeta }
  }
}

/// The transition declaration an element carries: per-property specs plus an
/// `all` catch-all. Applies to writes from the moment it is set; it does not
/// retroactively affect running tracks.
#[derive(Clone, Debug, Default)]
pub struct TransitionConfig {
  pub props: Vec<(AnimProp, TransitionSpec)>,
  pub all: Option<TransitionSpec>,
}

impl TransitionConfig {
  pub fn spec_for(&self, prop: AnimProp) -> Option<TransitionSpec> {
    self.props.iter().find(|(p, _)| *p == prop).map(|(_, s)| *s).or(self.all)
  }
}

/// Interpolation state of one running track.
#[derive(Clone, Copy, Debug)]
pub enum TrackState {
  Tween {
    from: f32,
    start_ms: f64,
  },
  /// Position and velocity (per second), integrated each frame.
  Spring {
    pos: f32,
    vel: f32,
  },
}

/// One running animation: a (node, prop) pair moving toward `to`.
#[derive(Debug)]
pub struct Track {
  pub node: u64,
  pub prop: AnimProp,
  pub to: f32,
  pub spec: TransitionSpec,
  pub state: TrackState,
  // Settle threshold, scaled to the animated distance so pixels and
  // unit-scale values (opacity) both settle promptly.
  eps: f32,
}

fn eps_for(distance: f32) -> f32 {
  (distance.abs().max(1.0) * 1e-3).max(1e-3)
}

impl Track {
  /// Advance to `now_ms` (`dt_ms` since the previous advance, for the
  /// spring). Returns the value to write and whether the track settled; a
  /// settled track reports the target exactly.
  pub fn advance(&mut self, now_ms: f64, dt_ms: f64) -> (f32, bool) {
    match (&mut self.state, self.spec) {
      (TrackState::Tween { from, start_ms }, TransitionSpec::Tween { duration_ms, curve }) => {
        let p = ((now_ms - *start_ms) / duration_ms as f64).clamp(0.0, 1.0) as f32;
        if p >= 1.0 {
          return (self.to, true);
        }
        (*from + (self.to - *from) * curve.eval(p), false)
      }
      (TrackState::Spring { pos, vel }, TransitionSpec::Spring { omega, zeta }) => {
        let dt = (dt_ms / 1000.0) as f32;
        let (x, v) = spring_step(*pos - self.to, *vel, omega, zeta, dt);
        *pos = self.to + x;
        *vel = v;
        if x.abs() < self.eps && v.abs() < self.eps * omega {
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

/// Exact step of the damped harmonic oscillator: position `x` relative to
/// the equilibrium, velocity `v` (units/s), over `dt` seconds. Closed-form
/// per branch, so large or irregular frame gaps stay stable.
fn spring_step(x: f32, v: f32, omega: f32, zeta: f32, dt: f32) -> (f32, f32) {
  if dt <= 0.0 {
    return (x, v);
  }
  if (zeta - 1.0).abs() < 1e-4 {
    // Critically damped: (x + (v + omega*x) t) e^(-omega t).
    let e = (-omega * dt).exp();
    let b = v + omega * x;
    let nx = (x + b * dt) * e;
    let nv = (v - omega * b * dt) * e;
    (nx, nv)
  } else if zeta < 1.0 {
    // Underdamped: x(t) = e^(-zeta omega t) (x cos wd t + B sin wd t) with
    // B = (v + zeta omega x)/wd; v(t) is the product-rule derivative.
    let wd = omega * (1.0 - zeta * zeta).sqrt();
    let e = (-zeta * omega * dt).exp();
    let (sin, cos) = (wd * dt).sin_cos();
    let b = (v + zeta * omega * x) / wd;
    let nx = e * (x * cos + b * sin);
    let nv = -zeta * omega * nx + e * wd * (b * cos - x * sin);
    (nx, nv)
  } else {
    // Overdamped: two real decay rates r1, r2.
    let s = (zeta * zeta - 1.0).sqrt();
    let r1 = -omega * (zeta - s);
    let r2 = -omega * (zeta + s);
    let c2 = (v - r1 * x) / (r2 - r1);
    let c1 = x - c2;
    let e1 = (r1 * dt).exp();
    let e2 = (r2 * dt).exp();
    (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
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
}

impl Transitions {
  pub fn is_empty(&self) -> bool {
    self.tracks.is_empty()
  }

  /// Start or retarget the track for (node, prop). `current` is the
  /// property's present value (the from-value for a fresh or restarted
  /// tween); a running spring keeps its position and velocity and only moves
  /// its equilibrium.
  pub fn retarget(&mut self, node: u64, prop: AnimProp, current: f32, to: f32, spec: TransitionSpec) {
    let now = self.now_ms;
    if let Some(t) = self.tracks.iter_mut().find(|t| t.node == node && t.prop == prop) {
      t.to = to;
      t.eps = eps_for(to - current);
      let keep_spring_state = matches!((&t.state, spec), (TrackState::Spring { .. }, TransitionSpec::Spring { .. }));
      t.spec = spec;
      if !keep_spring_state {
        t.state = match spec {
          TransitionSpec::Tween { .. } => TrackState::Tween { from: current, start_ms: now },
          TransitionSpec::Spring { .. } => TrackState::Spring { pos: current, vel: 0.0 },
        };
      }
      return;
    }
    if to == current {
      return;
    }
    if self.tracks.is_empty() {
      self.last_ms = now;
    }
    let state = match spec {
      TransitionSpec::Tween { .. } => TrackState::Tween { from: current, start_ms: now },
      TransitionSpec::Spring { .. } => TrackState::Spring { pos: current, vel: 0.0 },
    };
    self.tracks.push(Track { node, prop, to, spec, state, eps: eps_for(to - current) });
  }

  /// Drop the track for (node, prop), if any: a non-animated write to the
  /// property took over (null reset, non-numeric value).
  pub fn cancel(&mut self, node: u64, prop: AnimProp) {
    self.tracks.retain(|t| !(t.node == node && t.prop == prop));
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
  pub fn anim_value(&self, prop: AnimProp) -> Option<f32> {
    use AnimProp::*;
    let detached = !self.has_layout();
    match (&self.kind, prop) {
      (ElementKind::View(v), X) => Some(v.translate.map(|t| t.x).unwrap_or(0.0)),
      (ElementKind::View(v), Y) => Some(v.translate.map(|t| t.y).unwrap_or(0.0)),
      (ElementKind::View(v), Opacity) => Some(v.opacity.unwrap_or(1.0)),
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
    }
  }

  /// Write an animatable property through its typed setter, returning the
  /// setter's damage. Must accept exactly the (kind, prop) pairs
  /// `anim_value` answers; anything else is a no-op (`Damage::None`).
  pub fn set_anim_value(&mut self, prop: AnimProp, v: f32) -> Damage {
    use AnimProp::*;
    let detached = !self.has_layout();
    match (&mut self.kind, prop) {
      (ElementKind::View(view), X) => view.set_x(v),
      (ElementKind::View(view), Y) => view.set_y(v),
      (ElementKind::View(view), Opacity) => view.set_opacity(v),
      (ElementKind::View(view), Rotate) => view.set_rotate(v),
      (ElementKind::View(view), RotateX) => view.set_rotate_x(v),
      (ElementKind::View(view), RotateY) => view.set_rotate_y(v),
      (ElementKind::View(view), Scale) => {
        view.set_scale_x(v);
        view.set_scale_y(v)
      }
      (ElementKind::View(view), ScaleX) => view.set_scale_x(v),
      (ElementKind::View(view), ScaleY) => view.set_scale_y(v),

      (ElementKind::Rectangle(r), X) if detached => r.set_x(v),
      (ElementKind::Rectangle(r), Y) if detached => r.set_y(v),
      (ElementKind::Rectangle(r), W) if detached => r.set_w(v),
      (ElementKind::Rectangle(r), H) if detached => r.set_h(v),
      (ElementKind::Rectangle(r), Radius) => r.set_radius([v.max(0.0); 4]),

      (ElementKind::Oval(o), X) if detached => o.set_x(v),
      (ElementKind::Oval(o), Y) if detached => o.set_y(v),
      (ElementKind::Oval(o), W) if detached => o.set_w(v),
      (ElementKind::Oval(o), H) if detached => o.set_h(v),

      (ElementKind::Text(t), X) if detached => t.set_x(v),
      (ElementKind::Text(t), Y) if detached => t.set_y(v),

      (ElementKind::Texture(t), X) if detached => t.set_x(v),
      (ElementKind::Texture(t), Y) if detached => t.set_y(v),
      (ElementKind::Texture(t), W) if detached => t.set_w(v),
      (ElementKind::Texture(t), H) if detached => t.set_h(v),

      (ElementKind::Path(p), X) if detached => p.set_x(v),
      (ElementKind::Path(p), Y) if detached => p.set_y(v),

      (ElementKind::Line(l), X1) if detached => l.set_x1(v),
      (ElementKind::Line(l), Y1) if detached => l.set_y1(v),
      (ElementKind::Line(l), X2) if detached => l.set_x2(v),
      (ElementKind::Line(l), Y2) if detached => l.set_y2(v),

      (kind, StrokeWidth) => match kind.paint_mut() {
        Some(p) => p.set_stroke_width(v.max(0.0)),
        None => Damage::None,
      },

      _ => Damage::None,
    }
  }
}
