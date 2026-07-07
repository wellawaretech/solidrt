use super::{as_pct_fraction, decode_radius, f32_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::OriginCoord;
use alloy::rendertree::View;

pub fn apply(view: &mut View, name: &str, value: &PropValue) -> Option<Damage> {
  Some(match name {
    "rotate" => view.set_rotate(f32_of(value, "rotate")),
    "scale" => view.set_scale(f32_of(value, "scale")),
    "scaleX" => view.set_scale_x(f32_of(value, "scaleX")),
    "scaleY" => view.set_scale_y(f32_of(value, "scaleY")),
    "rotateX" => view.set_rotate_x(f32_of(value, "rotateX")),
    "rotateY" => view.set_rotate_y(f32_of(value, "rotateY")),
    "perspective" => view.set_perspective(f32_of(value, "perspective")),
    "x" => view.set_x(f32_of(value, "x")),
    "y" => view.set_y(f32_of(value, "y")),
    "transformOrigin" => {
      let (x, y) = decode_origin(value);
      view.set_origin(x, y)
    }
    "opacity" => view.set_opacity(f32_of(value, "opacity")),
    "scrollX" => view.set_scroll_x(f32_of(value, "scrollX")),
    "scrollY" => view.set_scroll_y(f32_of(value, "scrollY")),
    "clipRadius" => view.set_clip_radius(decode_radius(value)),
    _ => return None,
  })
}

// transformOrigin: `[x, y]` sets the axes independently; a single value applies
// to both, except a directional keyword ("left"/"top"/...) which sets its own
// axis and leaves the other centered (matching CSS `transform-origin`). Each
// axis is a pixel number, a `pct(n)` fraction, or a position keyword.
fn decode_origin(value: &PropValue) -> (OriginCoord, OriginCoord) {
  if let Some(items) = value.as_list() {
    if items.len() != 2 {
      panic!("transformOrigin array must have 2 elements [x, y]");
    }
    return (decode_origin_axis(&items[0]), decode_origin_axis(&items[1]));
  }

  let center = OriginCoord::Fraction(0.5);
  if let Some(s) = value.as_str() {
    return match s {
      "left" => (OriginCoord::Fraction(0.0), center),
      "right" => (OriginCoord::Fraction(1.0), center),
      "top" => (center, OriginCoord::Fraction(0.0)),
      "bottom" => (center, OriginCoord::Fraction(1.0)),
      "center" => (center, center),
      _ => panic!("unknown transformOrigin keyword '{s}'"),
    };
  }

  let both = decode_origin_axis(value);
  (both, both)
}

// One axis of a transform origin: a pixel number, a `pct(n)` fraction, or a
// position keyword (left/top = 0, center = 0.5, right/bottom = 1).
fn decode_origin_axis(value: &PropValue) -> OriginCoord {
  if let Some(n) = value.as_f64() {
    return OriginCoord::Px(n as f32);
  }
  if let Some(f) = as_pct_fraction(value) {
    return OriginCoord::Fraction(f);
  }
  match value.as_str() {
    Some("left") | Some("top") => OriginCoord::Fraction(0.0),
    Some("center") => OriginCoord::Fraction(0.5),
    Some("right") | Some("bottom") => OriginCoord::Fraction(1.0),
    Some(other) => panic!("unknown transformOrigin keyword '{other}'"),
    None => panic!("transformOrigin value must be a number, pct(), or a keyword"),
  }
}
