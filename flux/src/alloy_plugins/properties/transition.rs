use super::describe;
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::{
  AnimKind, AnimProp, AnimValue, Curve, Endpoint, TransitionConfig, TransitionEntry, TransitionSpec,
};
use alloy::spatial::NodeMotion;

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
    "scrollX" => AnimProp::ScrollX,
    "scrollY" => AnimProp::ScrollY,
    "opacity" => AnimProp::Opacity,
    "originX" => AnimProp::OriginX,
    "originY" => AnimProp::OriginY,
    "perspective" => AnimProp::Perspective,
    "clipRadius" => AnimProp::ClipRadius,
    "srcX" => AnimProp::SrcX,
    "srcY" => AnimProp::SrcY,
    "srcW" => AnimProp::SrcW,
    "srcH" => AnimProp::SrcH,
    "onLength" => AnimProp::OnLength,
    "offLength" => AnimProp::OffLength,
    "dashOffset" => AnimProp::DashOffset,
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
    AnimProp::ScrollX => "scrollX",
    AnimProp::ScrollY => "scrollY",
    AnimProp::Opacity => "opacity",
    AnimProp::OriginX => "originX",
    AnimProp::OriginY => "originY",
    AnimProp::Perspective => "perspective",
    AnimProp::ClipRadius => "clipRadius",
    AnimProp::SrcX => "srcX",
    AnimProp::SrcY => "srcY",
    AnimProp::SrcW => "srcW",
    AnimProp::SrcH => "srcH",
    AnimProp::OnLength => "onLength",
    AnimProp::OffLength => "offLength",
    AnimProp::DashOffset => "dashOffset",
    AnimProp::Rotate => "rotate",
    AnimProp::RotateX => "rotateX",
    AnimProp::RotateY => "rotateY",
    AnimProp::Scale => "scale",
    AnimProp::ScaleX => "scaleX",
    AnimProp::ScaleY => "scaleY",
    AnimProp::StrokeWidth => "strokeWidth",
    AnimProp::Radius => "radius",
    AnimProp::Color => "color",
    AnimProp::Layout => "layout",
  }
}

/// Decodes the `transition` property value: an object keyed by animatable
/// property name (plus `all` as a catch-all), each value
/// `{ duration, bounce?, delay?, from?, exit? }` (a spring),
/// `{ duration, curve, delay?, from?, exit? }` (a tween), or a shorthand
/// string.
/// The spring is the default: naming a `curve` is what opts into a tween,
/// and a bare `{ duration }` is a critically damped (bounce 0) spring.
/// A bare string is the `all` catch-all: `transition="300ms ease-out"`.
/// A `stagger` key (ms) makes the element a stagger group for descendant
/// enters and exits; a `layout` key declares the layout slide
/// (`decode_layout`). Durations and delays are milliseconds. `null` clears
/// the declaration.
pub fn decode(value: &PropValue) -> Result<Option<Box<TransitionConfig>>, String> {
  if value.is_null() {
    return Ok(None);
  }
  if let Some(s) = value.as_str() {
    return Ok(Some(Box::new(TransitionConfig { all: Some(parse_shorthand("transition", s)?), ..Default::default() })));
  }
  let entries = value.as_map().ok_or_else(|| {
    format!("transition must be a shorthand string or an object keyed by property name, got {}", describe(value))
  })?;
  let mut config = TransitionConfig::default();
  let mut layout = None;
  for (key, entry_value) in entries {
    if key == "stagger" {
      config.stagger_ms = Some(decode_stagger(entry_value)?);
    } else if key == "all" {
      config.all = Some(decode_entry(key, entry_value, None)?);
    } else if key == "layout" {
      // Resolved after the loop: `true` borrows `all`, whichever is first.
      layout = Some(entry_value);
    } else {
      let prop = anim_prop(key).ok_or_else(|| format!("transition.{key}: '{key}' is not an animatable property"))?;
      config.props.push((prop, decode_entry(key, entry_value, Some(prop))?));
    }
  }
  if let Some(value) = layout {
    config.layout = decode_layout(value, config.all)?;
  }
  Ok(Some(Box::new(config)))
}

/// The `layout` key: the layout slide's motion
/// (okf/backlog/transition-layout-animations.md). An entry object or
/// shorthand of its own, or `true` to borrow the `all` entry's motion - an
/// error without one, since `all` never covers layout by itself (a bare
/// `transition="300ms"` on every button must not make buttons slide on
/// every reflow); `false` and `null` declare none. `from` and `exit` do not
/// apply: a slide's endpoints are the boxes layout gives the node.
fn decode_layout(value: &PropValue, all: Option<TransitionEntry>) -> Result<Option<TransitionEntry>, String> {
  if value.is_null() {
    return Ok(None);
  }
  if let Some(borrow) = value.as_bool() {
    if !borrow {
      return Ok(None);
    }
    return match all {
      Some(entry) => Ok(Some(entry)),
      None => {
        Err("transition.layout: true borrows the all entry's motion and there is no all; give layout its own".into())
      }
    };
  }
  if let Some(map) = value.as_map() {
    if let Some((key, _)) = map.iter().find(|(k, _)| matches!(k.as_str(), "from" | "exit")) {
      return Err(format!(
        "transition.layout: {key} does not apply to layout (a slide runs from the box the node had to the box it gets)"
      ));
    }
  }
  decode_entry("layout", value, None).map(Some)
}

/// One entry: a shorthand string or the spec object. `prop` is the entry's
/// property, `None` for the `all` catch-all (where `from` and `exit` are
/// rejected - which property they would seed is unanswerable).
fn decode_entry(key: &str, value: &PropValue, prop: Option<AnimProp>) -> Result<TransitionEntry, String> {
  let at = format!("transition.{key}");
  if let Some(s) = value.as_str() {
    return parse_shorthand(&at, s);
  }
  let map =
    value.as_map().ok_or_else(|| format!("{at} must be an object or a shorthand string, got {}", describe(value)))?;
  for (k, _) in map {
    if !matches!(k.as_str(), "duration" | "curve" | "bounce" | "delay" | "from" | "exit") {
      return Err(format!("{at}: unknown key '{k}' (expected duration, bounce, curve, delay, from or exit)"));
    }
  }
  let spec = decode_duration_spec(&at, value.get("duration"), value.get("curve"), value.get("bounce"))?;
  let delay_ms = decode_delay(&at, value.get("delay"))?;
  let endpoint = |key: &str| -> Result<Option<Endpoint>, String> {
    match value.get(key) {
      None => Ok(None),
      Some(v) => {
        let Some(prop) = prop else {
          return Err(format!("{at}: {key} is per-property; name the property instead of 'all'"));
        };
        decode_endpoint(&at, key, v, prop, value, spec, delay_ms).map(Some)
      }
    }
  };
  let from = endpoint("from")?;
  let exit = endpoint("exit")?;
  Ok(TransitionEntry { spec, delay_ms, from, exit })
}

/// A lifecycle endpoint (`from` at mount, `exit` at removal). The bare
/// value form plays the entry's motion; the object form
/// `{ value, duration?, curve?, bounce?, delay? }` gives that direction
/// its own, so an ease-out enter can pair with an ease-in exit. A field
/// left out is the entry's (`entry` is the raw entry object, `spec` and
/// `delay_ms` its decoded motion), except that naming a `curve` or a
/// `bounce` decides the kind outright: an ease-in exit on a spring entry
/// is a tween of the entry's duration, not a clash.
fn decode_endpoint(
  at: &str,
  key: &str,
  value: &PropValue,
  prop: AnimProp,
  entry: &PropValue,
  spec: TransitionSpec,
  delay_ms: f32,
) -> Result<Endpoint, String> {
  let (value, spec, delay_ms) =
    decode_endpoint_with(at, key, value, entry, spec, delay_ms, |at, key, v| decode_endpoint_value(at, key, v, prop))?;
  Ok(Endpoint { value, spec, delay_ms })
}

/// The endpoint decoder both trees share (the element properties and the
/// node components speak the same object form; only the value differs,
/// which `decode_value` reads): the endpoint's value with the motion it
/// resolved to - its own fields merged over the entry's by the rule above.
fn decode_endpoint_with<T>(
  at: &str,
  key: &str,
  value: &PropValue,
  entry: &PropValue,
  spec: TransitionSpec,
  delay_ms: f32,
  decode_value: impl Fn(&str, &str, &PropValue) -> Result<T, String>,
) -> Result<(T, TransitionSpec, f32), String> {
  let Some(map) = value.as_map() else {
    return Ok((decode_value(at, key, value)?, spec, delay_ms));
  };
  let at = format!("{at}.{key}");
  for (k, _) in map {
    if !matches!(k.as_str(), "value" | "duration" | "curve" | "bounce" | "delay") {
      return Err(format!("{at}: unknown key '{k}' (expected value, duration, bounce, curve or delay)"));
    }
  }
  let Some(inner) = value.get("value") else {
    return Err(format!("{at}: value is required (what to animate {key})"));
  };
  let endpoint_value = decode_value(&at, "value", inner)?;
  let own_kind = value.get("curve").is_some() || value.get("bounce").is_some();
  let inherited = |k: &str| if own_kind { None } else { entry.get(k) };
  let spec = decode_duration_spec(
    &at,
    value.get("duration").or(entry.get("duration")),
    value.get("curve").or(inherited("curve")),
    value.get("bounce").or(inherited("bounce")),
  )?;
  let delay_ms = match value.get("delay") {
    None => delay_ms,
    Some(d) => decode_delay(&at, Some(d))?,
  };
  Ok((endpoint_value, spec, delay_ms))
}

/// The duration + kind core of an entry object, from its three raw fields:
/// `duration` (ms) required, a `curve` makes it a tween, otherwise it is a
/// spring (`bounce` defaults to 0, critically damped). The two never mix.
fn decode_duration_spec(
  at: &str,
  duration: Option<&PropValue>,
  curve: Option<&PropValue>,
  bounce: Option<&PropValue>,
) -> Result<TransitionSpec, String> {
  let duration = match duration {
    None => return Err(format!("{at}: duration (ms) is required")),
    Some(v) => {
      let n = v.as_f64().ok_or_else(|| format!("{at}: duration must be a number of ms, got {}", describe(v)))? as f32;
      if !(n > 0.0 && n.is_finite()) {
        return Err(format!("{at}: duration must be a positive number of ms, got {n}"));
      }
      n
    }
  };
  match (curve, bounce) {
    (Some(_), Some(_)) => Err(format!("{at}: curve (tween) and bounce (spring) are mutually exclusive")),
    (Some(c), None) => Ok(TransitionSpec::Tween { duration_ms: duration, curve: decode_curve(at, c)? }),
    (None, bounce) => {
      let bounce = match bounce {
        None => 0.0,
        Some(v) => v.as_f64().ok_or_else(|| format!("{at}: bounce must be a number, got {}", describe(v)))? as f32,
      };
      if !(bounce > -1.0 && bounce <= 1.0) {
        return Err(format!("{at}: bounce must be in (-1, 1], got {bounce}"));
      }
      Ok(TransitionSpec::spring(duration, bounce))
    }
  }
}

/// The `stagger` key of a declaration, element or node: descendant
/// enters/exits beginning in the same frame under the declaring node get
/// index * stagger ms of extra delay. A positive number of ms.
pub fn decode_stagger(value: &PropValue) -> Result<f32, String> {
  let n = value
    .as_f64()
    .ok_or_else(|| format!("transition.stagger: must be a number of ms, got {}", describe(value)))? as f32;
  if !(n > 0.0 && n.is_finite()) {
    return Err(format!("transition.stagger: must be a positive number of ms, got {n}"));
  }
  Ok(n)
}

/// A node transition entry decoded: the motion its writes play, and the
/// lifecycle endpoints as lane vectors (the component's lane count, checked
/// here) with the motion each resolved to.
#[derive(Debug)]
pub struct NodeEntryDecoded {
  pub motion: NodeMotion,
  pub from: Option<(Vec<f32>, NodeMotion)>,
  pub exit: Option<(Vec<f32>, NodeMotion)>,
}

/// The motion alone: what the node `all` catch-all speaks - `{ duration,
/// bounce?, delay? }` (a spring), `{ duration, curve, delay? }` (a tween)
/// or the shorthand string; `from` and `exit` are rejected there (which
/// component they would seed is unanswerable).
pub fn decode_node_motion(at: &str, value: &PropValue) -> Result<NodeMotion, String> {
  decode_node_entry(at, value, None).map(|d| d.motion)
}

/// A node transition entry: the element entry vocabulary whole, minus
/// nothing - `duration`, `curve`/`bounce`, `delay`, and the `from`/`exit`
/// endpoints of a component with `lanes` lanes (position and scale 3,
/// rotation 4 as a quaternion), each a bare lane array or the endpoint
/// object `{ value, duration?, curve?, bounce?, delay? }` merged by the
/// element rule (`decode_endpoint_with`). `lanes` None rejects the
/// endpoints (the `all` catch-all).
pub fn decode_node_entry(at: &str, value: &PropValue, lanes: Option<usize>) -> Result<NodeEntryDecoded, String> {
  if let Some(s) = value.as_str() {
    let entry = parse_shorthand(at, s)?;
    return Ok(NodeEntryDecoded {
      motion: NodeMotion { spec: entry.spec, delay_ms: entry.delay_ms },
      from: None,
      exit: None,
    });
  }
  let map =
    value.as_map().ok_or_else(|| format!("{at} must be an object or a shorthand string, got {}", describe(value)))?;
  for (k, _) in map {
    if !matches!(k.as_str(), "duration" | "curve" | "bounce" | "delay" | "from" | "exit") {
      return Err(format!("{at}: unknown key '{k}' (expected duration, bounce, curve, delay, from or exit)"));
    }
  }
  let spec = decode_duration_spec(at, value.get("duration"), value.get("curve"), value.get("bounce"))?;
  let delay_ms = decode_delay(at, value.get("delay"))?;
  let endpoint = |key: &str| -> Result<Option<(Vec<f32>, NodeMotion)>, String> {
    match value.get(key) {
      None => Ok(None),
      Some(v) => {
        let Some(lanes) = lanes else {
          return Err(format!("{at}: {key} is per-component; name the component instead of 'all'"));
        };
        let (lanes, spec, delay_ms) =
          decode_endpoint_with(at, key, v, value, spec, delay_ms, |at, key, v| decode_lanes(at, key, v, lanes))?;
        Ok(Some((lanes, NodeMotion { spec, delay_ms })))
      }
    }
  };
  let from = endpoint("from")?;
  let exit = endpoint("exit")?;
  Ok(NodeEntryDecoded { motion: NodeMotion { spec, delay_ms }, from, exit })
}

/// A node endpoint's value: the component's lanes, finite numbers.
fn decode_lanes(at: &str, key: &str, value: &PropValue, lanes: usize) -> Result<Vec<f32>, String> {
  let list = value
    .as_list()
    .filter(|l| l.len() == lanes)
    .ok_or_else(|| format!("{at}: {key} must be an array of {lanes} numbers, got {}", describe(value)))?;
  let mut out = Vec::with_capacity(lanes);
  for x in list {
    let n = x.as_f64().ok_or_else(|| format!("{at}: {key} must be an array of numbers, got {}", describe(x)))? as f32;
    if !n.is_finite() {
      return Err(format!("{at}: {key} must be finite, got {n}"));
    }
    out.push(n);
  }
  Ok(out)
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

/// A lifecycle endpoint's value: a number for the scalar properties; the
/// color property takes a CSS color string or a packed 0xRRGGBBAA number.
/// The slide lane takes no endpoint (`decode_layout` refuses `from`/`exit`
/// before a value is read), so the point arm is an error, not a decoder.
fn decode_endpoint_value(at: &str, key: &str, value: &PropValue, prop: AnimProp) -> Result<AnimValue, String> {
  match prop.kind() {
    AnimKind::Color => super::decode_color(value).map(AnimValue::Color).map_err(|e| format!("{at}: {key}: {e}")),
    AnimKind::Scalar => {
      let n = value.as_f64().ok_or_else(|| format!("{at}: {key} must be a number, got {}", describe(value)))? as f32;
      if !n.is_finite() {
        return Err(format!("{at}: {key} must be finite, got {n}"));
      }
      Ok(AnimValue::Scalar(n))
    }
    AnimKind::Point => Err(format!("{at}: {key} does not apply to layout")),
  }
}

/// The shorthand string: `"<duration>ms [curve] [<delay>ms]"`, e.g.
/// `"300ms"` (a bounce-0 spring), `"300ms ease-out"` (a tween),
/// `"300ms ease-out 100ms"` (delayed). The first time value is the
/// duration, the second the delay (CSS order); times are ms only. Bounce,
/// bezier control values, `from` and `exit` need the object form.
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
    } else if let Some(c) = Curve::named(token) {
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
  Ok(TransitionEntry { spec, delay_ms, from: None, exit: None })
}

fn decode_curve(at: &str, value: &PropValue) -> Result<Curve, String> {
  if let Some(name) = value.as_str() {
    return Curve::named(name).ok_or_else(|| {
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
