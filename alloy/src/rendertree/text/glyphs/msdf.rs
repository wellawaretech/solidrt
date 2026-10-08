// Distance-field cells: a glyph outline (outline.rs, y up, in texels at
// the cell kind's size) through msdfgen into an MTSDF cell, rgb the multi-channel
// field whose median is the edge and alpha the true distance, `range`
// texels across with 0.5 at the edge. One cell serves every zoom; a consumer
// decodes the field in its shader (the 2d sprite layer's `Atlas.sdf`).
//
// The outline's commands become msdfgen edges contour by contour under its
// own font importer's rules: a line or quadratic ending on the pen is
// dropped, a cubic too unless its controls span area, and every contour
// closes back to its start. msdfgen's inside is the right-hand side of
// travel, the TrueType convention (outer contours clockwise with y up); a
// font wound the other way (CFF) shows in its largest contour's signed area
// and is reversed whole. The cell box is the outline's bounds padded by
// half the range on every side, where the field reaches its outside value,
// rounded out to texels: the padding an atlas would need is in the cell.
use super::cells::{Cell, BYTES_PER_TEXEL};
use super::ffi;
use std::ffi::c_int;
use zeno::{Command, Point};

/// msdfgen's corner threshold for edge colouring, radians: edges meeting at
/// a sharper angle than this get different channels (the generator's own
/// default, about 172 degrees).
const CORNER_ANGLE_THRESHOLD: f64 = 3.0;
/// The field's byte range: 0 outside, 255 inside, the edge in the middle.
const FIELD_MAX: f32 = 255.0;

/// The MTSDF cell of `outline` with `range` texels of distance across the
/// field. A blank outline (a space) is an empty cell; None when msdfgen
/// rejects the shape or the range leaves no texel to draw in.
pub fn msdf_cell(glyph: u16, outline: &[Command], range: f32) -> Option<Cell> {
  let mut shape = Shape::new();
  shape.add_outline(outline);
  if counter_clockwise(outline) {
    shape.reverse();
  }
  shape.normalize();
  let Some([l, b, r, t]) = shape.bounds() else {
    return Some(Cell { key: glyph, width: 0, height: 0, left: 0, top: 0, pixels: Vec::new() });
  };
  let pad = range as f64 / 2.0;
  let left = (l - pad).floor();
  let bottom = (b - pad).floor();
  let right = (r + pad).ceil();
  let top = (t + pad).ceil();
  let width = (right - left) as u32;
  let height = (top - bottom) as u32;
  if width == 0 || height == 0 {
    return None;
  }
  let mut field = vec![0f32; width as usize * height as usize * BYTES_PER_TEXEL];
  if !shape.generate(&mut field, width, height, (-left, -bottom), range as f64) {
    return None;
  }
  let pixels = field.iter().map(|v| (v.clamp(0.0, 1.0) * FIELD_MAX).round() as u8).collect();
  Some(Cell { key: glyph, width, height, left: left as i32, top: top as i32, pixels })
}

/// Whether the outline's largest contour runs counter-clockwise (y up): the
/// CFF convention, the mirror of the winding msdfgen's inside assumes. The
/// largest contour is an outer one, so its sense is the font's.
fn counter_clockwise(outline: &[Command]) -> bool {
  let mut dominant = 0.0f64;
  let mut area = 0.0f64;
  let mut start = Point::default();
  let mut prev = Point::default();
  let end_contour = |area: &mut f64, prev: Point, start: Point, dominant: &mut f64| {
    *area += shoelace(prev, start);
    if area.abs() > dominant.abs() {
      *dominant = *area;
    }
    *area = 0.0;
  };
  for &command in outline {
    match command {
      Command::MoveTo(p) => {
        end_contour(&mut area, prev, start, &mut dominant);
        start = p;
        prev = p;
      }
      Command::LineTo(p) => {
        area += shoelace(prev, p);
        prev = p;
      }
      Command::QuadTo(c, p) => {
        area += shoelace(prev, c) + shoelace(c, p);
        prev = p;
      }
      Command::CurveTo(c0, c1, p) => {
        area += shoelace(prev, c0) + shoelace(c0, c1) + shoelace(c1, p);
        prev = p;
      }
      Command::Close => {
        area += shoelace(prev, start);
        prev = start;
      }
    }
  }
  end_contour(&mut area, prev, start, &mut dominant);
  dominant > 0.0
}

/// One term of the shoelace sum: twice the signed area the segment a-b
/// sweeps with the origin, positive counter-clockwise with y up.
fn shoelace(a: Point, b: Point) -> f64 {
  a.x as f64 * b.y as f64 - b.x as f64 * a.y as f64
}

fn same(a: Point, b: Point) -> bool {
  a.x == b.x && a.y == b.y
}

/// An msdfgen shape in the shim, freed with the value.
struct Shape {
  raw: *mut ffi::MsdfShape,
  start: Point,
  pen: Point,
  /// Edges added to the current contour, 0 before its first.
  edges: usize,
}

impl Shape {
  fn new() -> Self {
    Self { raw: unsafe { ffi::msdf_shape_new() }, start: Point::default(), pen: Point::default(), edges: 0 }
  }

  fn add_outline(&mut self, outline: &[Command]) {
    for &command in outline {
      match command {
        Command::MoveTo(p) => {
          self.close();
          self.start = p;
          self.pen = p;
        }
        Command::LineTo(p) => {
          if !same(p, self.pen) {
            self.begin_edge();
            unsafe { ffi::msdf_shape_line(self.raw, self.pen.x as f64, self.pen.y as f64, p.x as f64, p.y as f64) };
            self.pen = p;
          }
        }
        Command::QuadTo(c, p) => {
          if !same(p, self.pen) {
            self.begin_edge();
            unsafe {
              ffi::msdf_shape_quad(
                self.raw,
                self.pen.x as f64,
                self.pen.y as f64,
                c.x as f64,
                c.y as f64,
                p.x as f64,
                p.y as f64,
              )
            };
            self.pen = p;
          }
        }
        Command::CurveTo(c0, c1, p) => {
          let spans_area = shoelace(Point::new(c0.x - p.x, c0.y - p.y), Point::new(c1.x - p.x, c1.y - p.y)) != 0.0;
          if !same(p, self.pen) || spans_area {
            self.begin_edge();
            unsafe {
              ffi::msdf_shape_cubic(
                self.raw,
                self.pen.x as f64,
                self.pen.y as f64,
                c0.x as f64,
                c0.y as f64,
                c1.x as f64,
                c1.y as f64,
                p.x as f64,
                p.y as f64,
              )
            };
            self.pen = p;
          }
        }
        Command::Close => self.close(),
      }
    }
    self.close();
  }

  // A contour begins with its first edge, so a bare move adds nothing.
  fn begin_edge(&mut self) {
    if self.edges == 0 {
      unsafe { ffi::msdf_shape_contour(self.raw) };
    }
    self.edges += 1;
  }

  // Close the contour in progress back to its start.
  fn close(&mut self) {
    if self.edges > 0 && !same(self.pen, self.start) {
      let (pen, start) = (self.pen, self.start);
      unsafe { ffi::msdf_shape_line(self.raw, pen.x as f64, pen.y as f64, start.x as f64, start.y as f64) };
    }
    self.pen = self.start;
    self.edges = 0;
  }

  fn reverse(&mut self) {
    unsafe { ffi::msdf_shape_reverse(self.raw) };
  }

  fn normalize(&mut self) {
    unsafe { ffi::msdf_shape_normalize(self.raw) };
  }

  /// Left, bottom, right, top (y up); None for a shape without edges.
  fn bounds(&mut self) -> Option<[f64; 4]> {
    let mut bounds = [0.0f64; 4];
    let [l, b, r, t] = &mut bounds;
    let found = unsafe { ffi::msdf_shape_bounds(self.raw, l, b, r, t) };
    (found != 0).then_some(bounds)
  }

  /// Generate into `field` (width * height rgba floats, rows top first):
  /// texels at scale 1 after `translate`, `range` texels across.
  fn generate(&mut self, field: &mut [f32], width: u32, height: u32, translate: (f64, f64), range: f64) -> bool {
    debug_assert_eq!(field.len(), width as usize * height as usize * BYTES_PER_TEXEL);
    let ok = unsafe {
      ffi::msdf_generate_mtsdf(
        self.raw,
        CORNER_ANGLE_THRESHOLD,
        field.as_mut_ptr(),
        width as c_int,
        height as c_int,
        1.0,
        translate.0,
        translate.1,
        range,
      )
    };
    ok != 0
  }
}

impl Drop for Shape {
  fn drop(&mut self) {
    unsafe { ffi::msdf_shape_free(self.raw) };
  }
}
