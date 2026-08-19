use super::describe;
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::{AnimProp, AnimValue, Curve, TransitionConfig, TransitionEntry, TransitionSpec};

// The `transition` property (okf/done/native-transitions.md): decodes the
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
/// `{ duration, bounce?, delay?, from? }` (a spring),
/// `{ duration, curve, delay?, from? }` (a tween), or a shorthand string.
/// The spring is the default: naming a `curve` is what opts into a tween,
/// and a bare `{ duration }` is a critically damped (bounce 0) spring.
/// A bare string is the `all` catch-all: `transition="300ms ease-out"`.
/// Durations and delays are milliseconds. `null` clears the declaration.
pub fn decode(value: &PropValue) -> Result<Option<Box<TransitionConfig>>, String> {
  if value.is_null() {
    return Ok(None);
  }
  if let Some(s) = value.as_str() {
    return Ok(Some(Box::new(TransitionConfig { props: vec![], all: Some(parse_shorthand("transition", s)?) })));
  }
  let entries = value.as_map().ok_or_else(|| {
    format!("transition must be a shorthand string or an object keyed by property name, got {}", describe(value))
  })?;
  let mut config = TransitionConfig::default();
  for (key, entry_value) in entries {
    if key == "all" {
      config.all = Some(decode_entry(key, entry_value, None)?);
    } else {
      let prop = anim_prop(key).ok_or_else(|| format!("transition.{key}: '{key}' is not an animatable property"))?;
      config.props.push((prop, decode_entry(key, entry_value, Some(prop))?));
    }
  }
  Ok(Some(Box::new(config)))
}

/// One entry: a shorthand string or the spec object. `prop` is the entry's
/// property, `None` for the `all` catch-all (where `from` is rejected -
/// which property it would seed is unanswerable).
fn decode_entry(key: &str, value: &PropValue, prop: Option<AnimProp>) -> Result<TransitionEntry, String> {
  let at = format!("transition.{key}");
  if let Some(s) = value.as_str() {
    return parse_shorthand(&at, s);
  }
  let map =
    value.as_map().ok_or_else(|| format!("{at} must be an object or a shorthand string, got {}", describe(value)))?;
  for (k, _) in map {
    if !matches!(k.as_str(), "duration" | "curve" | "bounce" | "delay" | "from") {
      return Err(format!("{at}: unknown key '{k}' (expected duration, bounce, curve, delay or from)"));
    }
  }
  let duration = match value.get("duration") {
    None => return Err(format!("{at}: duration (ms) is required")),
    Some(v) => {
      let n = v.as_f64().ok_or_else(|| format!("{at}: duration must be a number of ms, got {}", describe(v)))? as f32;
      if !(n > 0.0 && n.is_finite()) {
        return Err(format!("{at}: duration must be a positive number of ms, got {n}"));
      }
      n
    }
  };
  // The kind is inferred: a `curve` makes it a tween, otherwise it is a
  // spring (`bounce` defaults to 0, critically damped). The two never mix.
  let spec = match (value.get("curve"), value.get("bounce")) {
    (Some(_), Some(_)) => return Err(format!("{at}: curve (tween) and bounce (spring) are mutually exclusive")),
    (Some(c), None) => TransitionSpec::Tween { duration_ms: duration, curve: decode_curve(&at, c)? },
    (None, bounce) => {
      let bounce = match bounce {
        None => 0.0,
        Some(v) => v.as_f64().ok_or_else(|| format!("{at}: bounce must be a number, got {}", describe(v)))? as f32,
      };
      if !(bounce > -1.0 && bounce <= 1.0) {
        return Err(format!("{at}: bounce must be in (-1, 1], got {bounce}"));
      }
      TransitionSpec::spring(duration, bounce)
    }
  };
  let delay_ms = decode_delay(&at, value.get("delay"))?;
  let from = match value.get("from") {
    None => None,
    Some(v) => {
      let Some(prop) = prop else {
        return Err(format!("{at}: from is per-property; name the property instead of 'all'"));
      };
      Some(decode_from(&at, v, prop)?)
    }
  };
  Ok(TransitionEntry { spec, delay_ms, from })
}

fn decode_delay(at: &str, value: Option<&PropValue>) -> Result<f32, String> {
  match value {
    None => Ok(0.0),
    Some(v) => {
      let n = v.as_f64().ok_or_else(|| format!("{at}: delay must be a number of ms, got {}", describe(v)))? as f32;
      if !(n >= 0.0 && n.is_finite()) {
        return Err(format!("{at}: delay must be a non-negative number of ms, got {n}"));
      }
      Ok(n)
    }
  }
}

/// The mount-time from-value: a number for the scalar properties; the color
/// property takes a CSS color string or a packed 0xRRGGBBAA number.
fn decode_from(at: &str, value: &PropValue, prop: AnimProp) -> Result<AnimValue, String> {
  if prop == AnimProp::Color {
    return super::decode_color(value).map(AnimValue::Color).map_err(|e| format!("{at}: from: {e}"));
  }
  let n = value.as_f64().ok_or_else(|| format!("{at}: from must be a number, got {}", describe(value)))? as f32;
  if !n.is_finite() {
    return Err(format!("{at}: from must be finite, got {n}"));
  }
  Ok(AnimValue::Scalar(n))
}

/// The shorthand string: `"<duration>ms [curve] [<delay>ms]"`, e.g.
/// `"300ms"` (a bounce-0 spring), `"300ms ease-out"` (a tween),
/// `"300ms ease-out 100ms"` (delayed). The first time value is the
/// duration, the second the delay (CSS order); times are ms only. Bounce,
/// bezier control values and `from` need the object form.
fn parse_shorthand(at: &str, s: &str) -> Result<TransitionEntry, String> {
  let mut duration: Option<f32> = None;
  let mut delay: Option<f32> = None;
  let mut curve: Option<Curve> = None;
  for token in s.split_whitespace() {
    if let Some(ms) = token.strip_suffix("ms").and_then(|n| n.parse::<f32>().ok()) {
      if duration.is_none() {
        duration = Some(ms);
      } else if delay.is_none() {
        delay = Some(ms);
      } else {
        return Err(format!("{at}: too many time values in \"{s}\" (duration, then an optional delay)"));
      }
    } else if let Some(c) = named_curve(token) {
      if curve.is_some() {
        return Err(format!("{at}: more than one curve in \"{s}\""));
      }
      curve = Some(c);
    } else {
      return Err(format!(
        "{at}: cannot read \"{token}\" in \"{s}\"; expected \"<duration>ms [curve] [<delay>ms]\", e.g. \"300ms ease-out\""
      ));
    }
  }
  let Some(duration) = duration else {
    return Err(format!("{at}: a duration like \"300ms\" is required, got \"{s}\""));
  };
  if !(duration > 0.0 && duration.is_finite()) {
    return Err(format!("{at}: duration must be a positive number of ms, got {duration}"));
  }
  let delay_ms = delay.unwrap_or(0.0);
  if !(delay_ms >= 0.0 && delay_ms.is_finite()) {
    return Err(format!("{at}: delay must be a non-negative number of ms, got {delay_ms}"));
  }
  let spec = match curve {
    Some(curve) => TransitionSpec::Tween { duration_ms: duration, curve },
    None => TransitionSpec::spring(duration, 0.0),
  };
  Ok(TransitionEntry { spec, delay_ms, from: None })
}

/// The CSS named curves, by their bezier control points.
fn named_curve(name: &str) -> Option<Curve> {
  Some(match name {
    "linear" => Curve::Linear,
    "ease" => Curve::Bezier(0.25, 0.1, 0.25, 1.0),
    "ease-in" => Curve::Bezier(0.42, 0.0, 1.0, 1.0),
    "ease-out" => Curve::Bezier(0.0, 0.0, 0.58, 1.0),
    "ease-in-out" => Curve::Bezier(0.42, 0.0, 0.58, 1.0),
    _ => return None,
  })
}

fn decode_curve(at: &str, value: &PropValue) -> Result<Curve, String> {
  if let Some(name) = value.as_str() {
    return named_curve(name).ok_or_else(|| {
      format!(
        "{at}: unknown curve \"{name}\"; expected linear, ease, ease-in, ease-out, ease-in-out or [x1, y1, x2, y2]"
      )
    });
  }
  if let Some(list) = value.as_list() {
    if list.len() != 4 {
      return Err(format!("{at}: a bezier curve is [x1, y1, x2, y2], got {} entries", list.len()));
    }
    let mut c = [0.0f32; 4];
    for (i, item) in list.iter().enumerate() {
      c[i] =
        item.as_f64().ok_or_else(|| format!("{at}: bezier control values must be numbers, got {}", describe(item)))?
          as f32;
    }
    // x controls outside [0, 1] make the curve non-solvable for progress.
    if !(0.0..=1.0).contains(&c[0]) || !(0.0..=1.0).contains(&c[2]) {
      return Err(format!("{at}: bezier x1/x2 must be in [0, 1], got {} and {}", c[0], c[2]));
    }
    return Ok(Curve::Bezier(c[0], c[1], c[2], c[3]));
  }
  Err(format!("{at}: curve must be a name or [x1, y1, x2, y2], got {}", describe(value)))
}
