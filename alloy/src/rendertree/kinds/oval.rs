use super::dash::{dashed_path, oval_outline, walked_length, Dash, Piece};
use super::{PaintState, ShadowState};
use crate::impellers::{ClipOperation, DisplayListBuilder, DrawStyle, Point, Rect, Size};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};

#[derive(Clone, Debug, Default)]
pub struct Oval {
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  pub on_length: Option<f32>,
  pub off_length: Option<f32>,
  pub dash_offset: Option<f32>,
  pub path_length: Option<f32>,
  pub paint: PaintState,
  pub shadow: Option<ShadowState>,
}

impl Buildable for Oval {
  // A stroke paints inside the box, on the same rule as `Rectangle::build`:
  // the drawn oval is the box inset by half the stroke width, so the stroke's
  // outer edge lands on the box edge instead of straddling it.
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let rect = self.geometry(ctx.size);
    // The shadow paints first, under the shape, cast by the outer box like
    // `Rectangle::build`: offset, grown by the spread on both axes, and the
    // casting oval clipped out (CSS: an outer shadow never paints beneath
    // its own shape).
    if let Some(shadow) = &self.shadow {
      let spread = shadow.spread;
      let cast = Rect::new(
        Point::new(rect.origin.x + shadow.dx - spread, rect.origin.y + shadow.dy - spread),
        Size::new((rect.size.width + spread * 2.0).max(0.0), (rect.size.height + spread * 2.0).max(0.0)),
      );
      builder.save();
      builder.clip_oval(&rect, ClipOperation::Difference);
      builder.draw_oval(&cast, &shadow.to_paint());
      builder.restore();
    }
    let path = self.stroke_path(ctx.size);
    match self.dashed_outline(ctx.size) {
      // Dashing is a stroke property: the fill keeps the whole inset oval,
      // the stroke gets its dashed pieces. Both paints are built from the
      // authored box, so a box-relative gradient stays anchored to the
      // element rather than to the inset stroke path.
      Some((outline, dash)) => {
        if self.fills() {
          let mut fill = self.paint.to_paint_in(&rect);
          fill.set_draw_style(DrawStyle::Fill);
          builder.draw_oval(&path, &fill);
        }
        let mut stroke = self.paint.to_paint_in(&rect);
        stroke.set_draw_style(DrawStyle::Stroke);
        builder.draw_path(&dashed_path(outline.into_iter(), dash), &stroke);
      }
      None => {
        builder.draw_oval(&path, &self.paint.to_paint_in(&rect));
      }
    }
  }
}

impl Oval {
  fn fills(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Fill | DrawStyle::StrokeAndFill)
  }

  fn strokes(&self) -> bool {
    matches!(self.paint.draw_style, DrawStyle::Stroke | DrawStyle::StrokeAndFill)
  }

  // The authored box, its x/y/w/h resolved against the layout size.
  fn geometry(&self, size: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(size.width), self.h.unwrap_or(size.height)),
    )
  }

  // The drawn oval: the box inset by the stroke inset.
  fn stroke_path(&self, size: Size) -> Rect {
    let rect = self.geometry(size);
    let d = self.paint.stroke_inset(rect.size.width, rect.size.height);
    Rect::new(
      Point::new(rect.origin.x + d, rect.origin.y + d),
      Size::new(rect.size.width - d * 2.0, rect.size.height - d * 2.0),
    )
  }

  // The dashed stroke, when the paint strokes and dashes: the inset outline
  // as walker pieces (four quarter arcs), and the pattern in local units,
  // a declared `pathLength` (SVG) mapping the author's units onto the
  // outline's length. As on `Rectangle`, a dash's cap never leaves the box.
  pub(crate) fn dashed_outline(&self, size: Size) -> Option<(Vec<Piece>, Dash)> {
    if !self.strokes() {
      return None;
    }
    let dash = Dash::new(self.on_length, self.off_length, self.dash_offset)?;
    let outline = oval_outline(self.stroke_path(size));
    let dash = match self.path_length.filter(|declared| *declared > 0.0) {
      Some(declared) => dash.scaled(walked_length(outline.iter().cloned()) / declared)?,
      None => dash,
    };
    Some((outline, dash))
  }
}

// An oval has no intrinsic size: a layout oval is sized by the width/height
// layout props (w/h are detached-only geometry and never reach taffy).
impl Measurable for Oval {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    Size::new(ctx.known.width.unwrap_or(0.0), ctx.known.height.unwrap_or(0.0))
  }
}

impl Bounded for Oval {
  fn local_bounds(&self, fallback: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(fallback.width), self.h.unwrap_or(fallback.height)),
    )
  }
}

impl Oval {
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    self.x = v;
    Damage::Paint
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    self.y = v;
    Damage::Paint
  }
  pub fn set_w(&mut self, v: Option<f32>) -> Damage {
    self.w = v;
    Damage::Paint
  }
  pub fn set_h(&mut self, v: Option<f32>) -> Damage {
    self.h = v;
    Damage::Paint
  }
  // The dash props are paint-only: the outline is walked again at build.
  pub fn set_on_length(&mut self, v: Option<f32>) -> Damage {
    self.on_length = v;
    Damage::Paint
  }
  pub fn set_off_length(&mut self, v: Option<f32>) -> Damage {
    self.off_length = v;
    Damage::Paint
  }
  pub fn set_dash_offset(&mut self, v: Option<f32>) -> Damage {
    self.dash_offset = v;
    Damage::Paint
  }
  pub fn set_path_length(&mut self, v: Option<f32>) -> Damage {
    self.path_length = v;
    Damage::Paint
  }
  pub fn set_shadow(&mut self, v: Option<ShadowState>) -> Damage {
    self.shadow = v;
    Damage::Paint
  }

  pub fn initial_style() -> taffy::Style {
    taffy::Style { display: taffy::Display::Block, ..Default::default() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Oval(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Oval(self))
  }
}

impl Hittable for Oval {
  fn is_in_bounds(&self, pt: Point, ctx: &HitContext) -> bool {
    let ox = self.x.unwrap_or(0.0);
    let oy = self.y.unwrap_or(0.0);
    let ow = self.w.unwrap_or(ctx.size.width);
    let oh = self.h.unwrap_or(ctx.size.height);
    let cx = ox + ow / 2.0;
    let cy = oy + oh / 2.0;
    let dx = pt.x - cx;
    let dy = pt.y - cy;
    let rx = ow / 2.0;
    let ry = oh / 2.0;
    if rx <= 0.0 || ry <= 0.0 {
      return false;
    }
    let inside = (dx / rx) * (dx / rx) + (dy / ry) * (dy / ry) <= 1.0;

    // Strokes paint inside the box (see `build`), so the box is the outer edge
    // for every draw style; only a plain stroke has a hole, one stroke width in.
    match self.paint.draw_style {
      DrawStyle::Fill | DrawStyle::StrokeAndFill => inside,
      DrawStyle::Stroke => {
        if !inside {
          return false;
        }
        let sw = self.paint.stroke_width.max(0.0);
        let rx_inner = rx - sw;
        let ry_inner = ry - sw;
        if rx_inner <= 0.0 || ry_inner <= 0.0 {
          return true;
        }
        (dx / rx_inner) * (dx / rx_inner) + (dy / ry_inner) * (dy / ry_inner) >= 1.0
      }
    }
  }
}
