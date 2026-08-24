// The motion math shared by every native-transition consumer: the tween
// curve solver, the perceptual spring spec, and the closed-form damped
// oscillator step. Factored from the rendertree's transitions (which keep
// their lane tracks and manager) so the spatial arena's node transitions
// run on the same vocabulary and the same integrator - one `{ duration }` /
// `{ duration, bounce }` / `{ duration, curve }` model everywhere. Pure
// math: no tree, no arena, no engine types.

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

/// How one animated value moves toward a new target. Both forms take their
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

/// Exact step of the damped harmonic oscillator: position `x` relative to
/// the equilibrium, velocity `v` (units/s), over `dt` seconds. Closed-form
/// per branch, so large or irregular frame gaps stay stable.
pub fn spring_step(x: f32, v: f32, omega: f32, zeta: f32, dt: f32) -> (f32, f32) {
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
