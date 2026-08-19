use super::describe;
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::{AnimProp, Curve, TransitionConfig, TransitionSpec};

// The `transition` property (okf/backlog/native-transitions.md): decodes the
// JS declaration into the native TransitionConfig the rendertree consumes,
// and maps JSX property names onto the animatable-property ids. Decode only;
// the animation itself lives in alloy (rendertree/transitions.rs).

/// The animatable-property id for a JSX property name, `None` for names that
/// never animate. Whether the element's kind actually carries the property
/// is decided tree-side (Element::anim_value).
pub fn anim_prop(name: &str) -> Option<AnimProp> {
  Some(match name {
    "x" => AnimProp::X,
    "y" => AnimProp::Y,
    "w" => AnimProp::W,
    "h" => AnimProp::H,
    "x1" => AnimProp::X1,
    "y1" => AnimProp::Y1,
    "x2" => AnimProp::X2,
    "y2" => AnimProp::Y2,
    "opacity" => AnimProp::Opacity,
    "rotate" => AnimProp::Rotate,
    "rotateX" => AnimProp::RotateX,
    "rotateY" => AnimProp::RotateY,
    "scale" => AnimProp::Scale,
    "scaleX" => AnimProp::ScaleX,
    "scaleY" => AnimProp::ScaleY,
    "strokeWidth" => AnimProp::StrokeWidth,
    "radius" => AnimProp::Radius,
    "color" => AnimProp::Color,
    _ => return None,
  })
}

/// The JSX name for an animatable-property id, the reverse of `anim_prop`
/// (the onTransitionEnd payload speaks JSX names).
pub fn anim_prop_name(prop: AnimProp) -> &'static str {
  match prop {
    AnimProp::X => "x",
    AnimProp::Y => "y",
    AnimProp::W => "w",
    AnimProp::H => "h",
    AnimProp::X1 => "x1",
    AnimProp::Y1 => "y1",
    AnimProp::X2 => "x2",
    AnimProp::Y2 => "y2",
    AnimProp::Opacity => "opacity",
    AnimProp::Rotate => "rotate",
    AnimProp::RotateX => "rotateX",
    AnimProp::RotateY => "rotateY",
    AnimProp::Scale => "scale",
    AnimProp::ScaleX => "scaleX",
    AnimProp::ScaleY => "scaleY",
    AnimProp::StrokeWidth => "strokeWidth",
    AnimProp::Radius => "radius",
    AnimProp::Color => "color",
  }
}

/// Decodes the `transition` property value: an object keyed by animatable
/// property name (plus `all` as a catch-all), each value
/// `{ duration, bounce? }` (a spring) or `{ duration, curve }` (a tween).
/// The spring is the default: naming a `curve` is what opts into a tween,
/// and a bare `{ duration }` is a critically damped (bounce 0) spring.
/// Durations are milliseconds. `null` clears the declaration.
pub fn decode(value: &PropValue) -> Result<Option<Box<TransitionConfig>>, String> {
  if value.is_null() {
    return Ok(None);
  }
  let entries = value
    .as_map()
    .ok_or_else(|| format!("transition must be an object keyed by property name, got {}", describe(value)))?;
  let mut config = TransitionConfig::default();
  for (key, spec_value) in entries {
    let spec = decode_spec(key, spec_value)?;
    if key == "all" {
      config.all = Some(spec);
    } else {
      let prop = anim_prop(key).ok_or_else(|| format!("transition.{key}: '{key}' is not an animatable property"))?;
      config.props.push((prop, spec));
    }
  }
  Ok(Some(Box::new(config)))
}

fn decode_spec(key: &str, value: &PropValue) -> Result<TransitionSpec, String> {
  let map = value.as_map().ok_or_else(|| format!("transition.{key} must be an object, got {}", describe(value)))?;
  for (k, _) in map {
    if k != "duration" && k != "curve" && k != "bounce" {
      return Err(format!("transition.{key}: unknown key '{k}' (expected duration, bounce or curve)"));
    }
  }
  let duration = match value.get("duration") {
    None => return Err(format!("transition.{key}: duration (ms) is required")),
    Some(v) => {
      let n =
        v.as_f64().ok_or_else(|| format!("transition.{key}: duration must be a number of ms, got {}", describe(v)))?
          as f32;
      if !(n > 0.0 && n.is_finite()) {
        return Err(format!("transition.{key}: duration must be a positive number of ms, got {n}"));
      }
      n
    }
  };
  // The kind is inferred: a `curve` makes it a tween, otherwise it is a
  // spring (`bounce` defaults to 0, critically damped). The two never mix.
  match (value.get("curve"), value.get("bounce")) {
    (Some(_), Some(_)) => Err(format!("transition.{key}: curve (tween) and bounce (spring) are mutually exclusive")),
    (Some(c), None) => Ok(TransitionSpec::Tween { duration_ms: duration, curve: decode_curve(key, c)? }),
    (None, bounce) => {
      let bounce = match bounce {
        None => 0.0,
        Some(v) => {
          v.as_f64().ok_or_else(|| format!("transition.{key}: bounce must be a number, got {}", describe(v)))? as f32
        }
      };
      if !(bounce > -1.0 && bounce <= 1.0) {
        return Err(format!("transition.{key}: bounce must be in (-1, 1], got {bounce}"));
      }
      Ok(TransitionSpec::spring(duration, bounce))
    }
  }
}

fn decode_curve(key: &str, value: &PropValue) -> Result<Curve, String> {
  if let Some(name) = value.as_str() {
    // The CSS named curves, by their bezier control points.
    return Ok(match name {
      "linear" => Curve::Linear,
      "ease" => Curve::Bezier(0.25, 0.1, 0.25, 1.0),
      "ease-in" => Curve::Bezier(0.42, 0.0, 1.0, 1.0),
      "ease-out" => Curve::Bezier(0.0, 0.0, 0.58, 1.0),
      "ease-in-out" => Curve::Bezier(0.42, 0.0, 0.58, 1.0),
      other => {
        return Err(format!(
          "transition.{key}: unknown curve \"{other}\"; expected linear, ease, ease-in, ease-out, ease-in-out or [x1, y1, x2, y2]"
        ))
      }
    });
  }
  if let Some(list) = value.as_list() {
    if list.len() != 4 {
      return Err(format!("transition.{key}: a bezier curve is [x1, y1, x2, y2], got {} entries", list.len()));
    }
    let mut c = [0.0f32; 4];
    for (i, item) in list.iter().enumerate() {
      c[i] = item
        .as_f64()
        .ok_or_else(|| format!("transition.{key}: bezier control values must be numbers, got {}", describe(item)))?
        as f32;
    }
    // x controls outside [0, 1] make the curve non-solvable for progress.
    if !(0.0..=1.0).contains(&c[0]) || !(0.0..=1.0).contains(&c[2]) {
      return Err(format!("transition.{key}: bezier x1/x2 must be in [0, 1], got {} and {}", c[0], c[2]));
    }
    return Ok(Curve::Bezier(c[0], c[1], c[2], c[3]));
  }
  Err(format!("transition.{key}: curve must be a name or [x1, y1, x2, y2], got {}", describe(value)))
}
