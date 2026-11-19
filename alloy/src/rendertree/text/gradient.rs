// A run's gradient paint resolved for anvil's glyph program: the layer's
// pixels mapped to the gradient's parameter, and its stops sampled into a
// ramp (see gpu::RampGradient; anvil owns the affine toolkit, the ramp
// sampler and the linear and radial arithmetic, this resolver the box
// units and the layer's scale). What Impeller's color source did per
// paragraph draw, the program does per fragment, so gradient text draws
// as a gradient and not in a solid stand-in color. Coordinates: a box
// gradient resolves against the text's own box (its wrap width by its
// lines' height, at the text origin); an absolute one's coordinates are
// text-local pixels from that origin, its transform mapping gradient space
// into them as it does for every other element.
use crate::renderer::affine::{compose, invert, of_matrix, IDENTITY};
use crate::renderer::{Affine, Color, Point, RampGradient, Size};
use crate::rendertree::{Gradient, GradientStop, GradientUnits};

// The map from the unit square onto a box of `size` at the origin.
fn box_map(size: Size) -> Affine {
  [[size.width, 0.0, 0.0], [0.0, size.height, 0.0]]
}

// The stops as the parallel colors and offsets anvil's ramp takes.
fn split(stops: &[GradientStop]) -> (Vec<Color>, Vec<f32>) {
  (stops.iter().map(|s| s.color).collect(), stops.iter().map(|s| s.offset).collect())
}

/// `gradient` as the glyph program evaluates it over a layer whose pixel
/// (0, 0) is `box_origin` in text-local pixels, `scale` layer pixels per
/// logical pixel, the text's own box being `content` at the text origin.
/// None when the gradient cannot be evaluated (no stops, a degenerate
/// geometry), in which case the run draws in its solid color.
pub(crate) fn layer_gradient(
  gradient: &Gradient,
  content: Size,
  box_origin: Point,
  scale: f32,
) -> Option<RampGradient> {
  // Layer pixels to text-local pixels.
  let to_local = [[1.0 / scale, 0.0, box_origin.x], [0.0, 1.0 / scale, box_origin.y]];
  match gradient {
    Gradient::Linear { start, end, stops, tile, transform, units } => {
      let to_space = match units {
        GradientUnits::Absolute => invert(of_matrix(transform))?,
        GradientUnits::BoundingBox => invert(box_map(content))?,
      };
      let (colors, offsets) = split(stops);
      RampGradient::linear(*start, *end, &colors, &offsets, *tile, compose(to_space, to_local))
    }
    Gradient::Radial { center, radius, stops, tile, transform, units, circle } => {
      if *radius <= 0.0 || !radius.is_finite() {
        return None;
      }
      let (to_space, center, radius) = match units {
        GradientUnits::Absolute => (invert(of_matrix(transform))?, *center, *radius),
        // A true circle: center and radius in pixels, no stretch.
        GradientUnits::BoundingBox if *circle => (
          IDENTITY,
          Point::new(center.x * content.width, center.y * content.height),
          radius * content.width.min(content.height),
        ),
        // An ellipse: the box map stretches the unit circle.
        GradientUnits::BoundingBox => (invert(box_map(content))?, *center, *radius),
      };
      let (colors, offsets) = split(stops);
      RampGradient::radial(center, radius, &colors, &offsets, *tile, compose(to_space, to_local))
    }
  }
}
