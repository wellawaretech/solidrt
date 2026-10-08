// A glyph outline as zeno path commands: y up, in pixels at the size skrifa
// drew it at (the pen it draws into). The synthetic styles a face lacks are
// applied here before a mask or a field is made from the commands: a slant
// as a skew of the path, a bold or a darkening as FreeType's outline
// embolden (the algorithm every renderer uses for a faux bold, carried
// over from swash's port of it when swash's own outline type went).
use skrifa::outline::OutlinePen;
use zeno::{Angle, Command, Point, Transform};

/// Cosine of the turn at a point past which the embolden stops shifting it
/// along the bisector (FreeType's `-0xF000` in 16.16: a corner sharper
/// than about 160 degrees would send the shifted point off to infinity).
const EMBOLDEN_TURN_COS: f32 = -0.9396;

#[derive(Default, Clone, Debug)]
pub struct Outline {
  commands: Vec<Command>,
}

impl Outline {
  pub fn commands(&self) -> &[Command] {
    &self.commands
  }

  /// A glyph without ink (a space) draws no command.
  pub fn is_empty(&self) -> bool {
    self.commands.is_empty()
  }

  /// Slant the outline by `degrees` from vertical, the top leaning right.
  pub fn skew(&mut self, degrees: f32) {
    let transform = Transform::skew(Angle::from_degrees(degrees), Angle::ZERO);
    for command in &mut self.commands {
      *command = command.transform(&transform);
    }
  }

  /// Grow the outline by `strength` pixels on every side: every point moves
  /// outward along the bisector of its two edges, bounded at sharp corners
  /// (FreeType's `FT_Outline_EmboldenXY`, the same strength both ways).
  /// The winding is read off the whole outline, so counters shrink as the
  /// stems around them grow.
  pub fn embolden(&mut self, strength: f32) {
    let mut points: Vec<Point> = Vec::new();
    let mut contours: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for command in &self.commands {
      match command {
        Command::MoveTo(p) => {
          if points.len() > start {
            contours.push((start, points.len()));
          }
          start = points.len();
          points.push(*p);
        }
        Command::LineTo(p) => points.push(*p),
        Command::QuadTo(c, p) => points.extend([*c, *p]),
        Command::CurveTo(c0, c1, p) => points.extend([*c0, *c1, *p]),
        Command::Close => {
          if points.len() > start {
            contours.push((start, points.len()));
          }
          start = points.len();
        }
      }
    }
    if points.len() > start {
      contours.push((start, points.len()));
    }
    let counter_clockwise = signed_area(&points) > 0.0;
    for (from, to) in contours {
      embolden_contour(&mut points[from..to], counter_clockwise, strength);
    }
    let mut next = points.into_iter();
    let mut take = || next.next().expect("one shifted point per outline point");
    for command in &mut self.commands {
      *command = match command {
        Command::MoveTo(_) => Command::MoveTo(take()),
        Command::LineTo(_) => Command::LineTo(take()),
        Command::QuadTo(..) => Command::QuadTo(take(), take()),
        Command::CurveTo(..) => Command::CurveTo(take(), take(), take()),
        Command::Close => Command::Close,
      };
    }
  }
}

impl OutlinePen for Outline {
  fn move_to(&mut self, x: f32, y: f32) {
    self.commands.push(Command::MoveTo(Point::new(x, y)));
  }

  fn line_to(&mut self, x: f32, y: f32) {
    self.commands.push(Command::LineTo(Point::new(x, y)));
  }

  fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
    self.commands.push(Command::QuadTo(Point::new(cx0, cy0), Point::new(x, y)));
  }

  fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
    self.commands.push(Command::CurveTo(Point::new(cx0, cy0), Point::new(cx1, cy1), Point::new(x, y)));
  }

  fn close(&mut self) {
    self.commands.push(Command::Close);
  }
}

// Twice the signed area the closed polygon of `points` sweeps, positive
// counter-clockwise with y up: the sense of the outline's dominant contours.
fn signed_area(points: &[Point]) -> f32 {
  let Some(last) = points.last() else {
    return 0.0;
  };
  let mut area = 0.0;
  let mut prev = *last;
  for cur in points {
    area += (cur.y - prev.y) * (cur.x + prev.x);
    prev = *cur;
  }
  area
}

// FreeType's embolden over one closed contour of control points, every
// point treated alike (on or off the curve): walk the edges, and for each
// corner shift the run of coincident points at it along the bisector of
// the edge in and the edge out, by `strength` scaled for the corner's
// angle and capped by the shorter edge so a sharp corner does not spike.
fn embolden_contour(points: &mut [Point], counter_clockwise: bool, strength: f32) {
  if points.is_empty() {
    return;
  }
  let last = points.len() - 1;
  let step = |i: usize| if i < last { i + 1 } else { 0 };
  let mut i = last;
  let mut j = 0;
  let mut k = usize::MAX;
  let mut in_dir = Point::ZERO;
  let mut in_len = 0.0f32;
  let mut anchor = Point::ZERO;
  let mut anchor_len = 0.0f32;
  while j != i && i != k {
    let (out_dir, out_len);
    if j != k {
      let out = points[j] - points[i];
      let len = out.length();
      if len == 0.0 {
        j = step(j);
        continue;
      }
      out_dir = Point::new(out.x / len, out.y / len);
      out_len = len;
    } else {
      out_dir = anchor;
      out_len = anchor_len;
    }
    if in_len != 0.0 {
      if k == usize::MAX {
        k = i;
        anchor = in_dir;
        anchor_len = in_len;
      }
      let mut d = in_dir.x * out_dir.x + in_dir.y * out_dir.y;
      let shift = if d > EMBOLDEN_TURN_COS {
        d += 1.0;
        let mut sx = in_dir.y + out_dir.y;
        let mut sy = in_dir.x + out_dir.x;
        if counter_clockwise {
          sy = -sy;
        } else {
          sx = -sx;
        }
        let mut q = out_dir.x * in_dir.y - out_dir.y * in_dir.x;
        if !counter_clockwise {
          q = -q;
        }
        let l = in_len.min(out_len);
        if strength * q <= l * d {
          sx = sx * strength / d;
          sy = sy * strength / d;
        } else {
          sx = sx * l / q;
          sy = sy * l / q;
        }
        Point::new(sx, sy)
      } else {
        Point::ZERO
      };
      while i != j {
        points[i].x += strength + shift.x;
        points[i].y += strength + shift.y;
        i = step(i);
      }
    } else {
      i = j;
    }
    in_dir = out_dir;
    in_len = out_len;
    j = step(j);
  }
}
