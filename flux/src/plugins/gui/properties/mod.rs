// JSX property adapter.
//
// The binding layer between the JSX/web property protocol and the
// engine-agnostic rendertree. Every frontend convention lives here - camelCase
// names, CSS-style string enums ("row-reverse", "strokeAndFill"), font aliases
// ("mono"/"sans"), packed-u32 colors, the radius-as-array shape - and gets
// decoded into native values that are handed to rendertree setters. rendertree
// knows none of it. One file per element mirrors rendertree's own structure.
//
// apply_jsx returns the Damage the write caused (layout / paint / transform /
// none); that decision is owned by rendertree (each setter reports it) and just
// threaded back out here for RenderTree::try_edit to apply.

mod layout;
mod line;
mod oval;
mod paint;
mod path;
mod read;
mod rectangle;
mod text;
mod texture;
mod view;
mod window;

pub use read::{read_jsx, ReadValue};

use std::sync::mpsc::Sender;

use alloy::AlloyCommand;
use taffy::style::Position;

use crate::plugins::gui::value::PropValue;
use alloy::impellers::Color;
use alloy::rendertree::text_layout::{Clear, Side};
use alloy::rendertree::{BoundaryMode, Damage, Element, ElementKind, PointerEvents};

// Returns Ok(damage) on success; Err(message) for an unknown property or a
// value that does not decode, which the FFI caller surfaces as a throwable JS
// error rather than aborting the process. A single typo'd or unsupported prop
// must not take down the runtime.
//
// The "Unknown property" and "Detached-only" message prefixes are matched by
// core's renderer (setTreeProperty) to warn-and-continue on name-level
// rejections; every other Err - a bad VALUE for a known property - is
// rethrown there, per the throw-in-dev validation policy.
// `gpu_params` routes a texture `params` write straight to the GPU channel
// (Context::set_target_params in production; a stub in tests): params are
// target state, not element state, so the write produces no tree damage and
// the raster dirty flush paces any number of writes into one render per
// frame. Content damage covers snapshot consumers.
pub fn apply_jsx(
  el: &mut Element,
  name: &str,
  value: &PropValue,
  cmd_tx: &Sender<AlloyCommand>,
  gpu_params: &dyn Fn(u64, &[(String, alloy::ParamValue)]) -> Result<(), String>,
) -> Result<Damage, String> {
  // `position` is decoded here rather than in the layout style adapter because
  // it has a side effect beyond the taffy Style: it marks the element as a
  // positioning context used to resolve container-relative bounding boxes.
  if name == "position" {
    let position = match str_of(value, "position")? {
      "relative" => Position::Relative,
      "absolute" => Position::Absolute,
      v => return Err(format!("Unknown position value \"{v}\"; expected relative or absolute")),
    };
    el.set_position(position);
    return Ok(Damage::Layout);
  }

  // Element-level, kind-independent: marks a retained-paint boundary
  // (see Element::repaint_boundary). Does not affect layout; Damage::Paint
  // clears the node's own now-stale cache along with the ancestors'.
  if name == "repaintBoundary" {
    el.repaint_boundary = match value {
      PropValue::Null | PropValue::Bool(false) => BoundaryMode::None,
      PropValue::Bool(true) => BoundaryMode::Recording,
      PropValue::Text(s) if s == "snapshot" => BoundaryMode::Snapshot,
      PropValue::Text(s) if s == "snapshot-no-aa" => BoundaryMode::SnapshotNoAa,
      _ => {
        return Err(format!(
          "repaintBoundary must be a boolean, \"snapshot\", or \"snapshot-no-aa\", got {}",
          describe(value)
        ))
      }
    };
    return Ok(Damage::Paint);
  }

  // Element-level, kind-independent: as an inline atom of a <text>, float out
  // of the flow to one side (see Element::float). Layout: the owning text's
  // runs are re-collected by the post-write resync.
  if name == "float" {
    el.float = match value {
      PropValue::Null => None,
      _ => match str_of(value, "float")? {
        "left" => Some(Side::Left),
        "right" => Some(Side::Right),
        v => return Err(format!("Unknown float value \"{v}\"; expected left or right")),
      },
    };
    return Ok(Damage::Layout);
  }
  if name == "clear" {
    el.clear = match value {
      PropValue::Null => None,
      _ => match str_of(value, "clear")? {
        "left" => Some(Clear::Left),
        "right" => Some(Clear::Right),
        "both" => Some(Clear::Both),
        v => return Err(format!("Unknown clear value \"{v}\"; expected left, right or both")),
      },
    };
    return Ok(Damage::Layout);
  }

  // Element-level, kind-independent: controls hit testing (see hit.rs). Paint/hit
  // only, no layout invalidation. `pointerEvents` is inherited (like CSS): a
  // null clears any local override rather than forcing Auto, so components
  // that forward this prop even when the app never set it do not break
  // inheritance from an ancestor that did.
  if name == "pointerEvents" {
    let pointer_events = match value {
      PropValue::Null => None,
      _ => match str_of(value, "pointerEvents")? {
        "auto" => Some(PointerEvents::Auto),
        "none" => Some(PointerEvents::None),
        "all" => Some(PointerEvents::All),
        v => return Err(format!("Unknown pointerEvents value \"{v}\"; expected auto, none or all")),
      },
    };
    el.set_pointer_events(pointer_events);
    return Ok(Damage::None);
  }

  // Box-geometry vocabulary is detached-only: a layout element's geometry IS
  // its layout box (sized via the width/height layout props), so on a layout
  // element these names are rejected like unknown properties (the renderer
  // warns and continues) instead of silently painting geometry that diverges
  // from the box. View x/y are transforms, not box geometry, and stay.
  if el.has_layout() && detached_only_geometry(&el.kind, name) {
    return Err(format!("Detached-only property '{name}': a layout element's geometry is its layout box (size it with width/height), so this is available on the d-* form only"));
  }

  let handled = match &mut el.kind {
    ElementKind::Window(win) => window::apply(win, name, value, cmd_tx)?,
    ElementKind::View(view) => view::apply(view, name, value)?,
    ElementKind::Rectangle(rect) => rectangle::apply(rect, name, value)?,
    ElementKind::Oval(oval) => oval::apply(oval, name, value)?,
    ElementKind::Line(line) => line::apply(line, name, value)?,
    ElementKind::Path(path) => path::apply(path, name, value)?,
    ElementKind::Text(text) => text::apply(text, name, value)?,
    ElementKind::Span(span) => text::apply_span(span, name, value)?,
    ElementKind::Texture(tex) => texture::apply(tex, name, value, gpu_params)?,
  };
  if let Some(damage) = handled {
    return Ok(damage);
  }

  if let Some(paint) = el.kind.paint_mut() {
    if let Some(damage) = paint::apply(paint, name, value)? {
      return Ok(damage);
    }
  }

  if let Some(style) = el.style_mut() {
    if let Some(damage) = layout::apply(style, name, value)? {
      return Ok(damage);
    }
  }

  Err(format!("Unknown property '{name}'"))
}

fn detached_only_geometry(kind: &ElementKind, name: &str) -> bool {
  match kind {
    ElementKind::Rectangle(_) | ElementKind::Oval(_) | ElementKind::Text(_) | ElementKind::Texture(_) => {
      matches!(name, "x" | "y" | "w" | "h")
    }
    ElementKind::Path(_) => matches!(name, "x" | "y"),
    ElementKind::Line(_) => matches!(name, "x1" | "y1" | "x2" | "y2"),
    _ => false,
  }
}

// Renders a PropValue for error messages: scalars verbatim, composites
// summarized, so a bad value names itself instead of just its expected type.
pub(super) fn describe(value: &PropValue) -> String {
  match value {
    PropValue::Null => "null".into(),
    PropValue::Bool(b) => b.to_string(),
    PropValue::Number(n) => n.to_string(),
    PropValue::Text(s) => format!("\"{s}\""),
    PropValue::List(items) => format!("a list of {}", items.len()),
    PropValue::Map(_) => "an object".into(),
  }
}

// Shared value decoders, kept here so every per-element module reads the same.
pub(super) fn f32_of(value: &PropValue, what: &str) -> Result<f32, String> {
  value.as_f64().map(|n| n as f32).ok_or_else(|| format!("{what} must be a number, got {}", describe(value)))
}

// The branded `pct(n)` value from JS: { __unit: "pct", v: n }. Returns the
// fraction (n / 100), or None for any value that is not a pct. Lets a
// percentage cross the boundary as a first-class value that no consumer has to
// string-parse; a bare number stays pixels.
pub(super) fn as_pct_fraction(value: &PropValue) -> Result<Option<f32>, String> {
  if value.get("__unit").and_then(PropValue::as_str) == Some("pct") {
    let v = value
      .get("v")
      .and_then(PropValue::as_f64)
      .ok_or_else(|| "pct() value must be a number".to_string())? as f32;
    Ok(Some(v / 100.0))
  } else {
    Ok(None)
  }
}

pub(super) fn str_of<'a>(value: &'a PropValue, what: &str) -> Result<&'a str, String> {
  value.as_str().ok_or_else(|| format!("{what} must be a string, got {}", describe(value)))
}

// { name: number | number[] } shader uniform values, dispatched by the
// shader's declared GLSL type in alloy (float/int scalar, vec2/3/4, mat4 as
// 16 numbers). Null clears (an empty set); a non-numeric entry is an error.
pub(super) fn decode_params(value: &PropValue) -> Result<Vec<(String, alloy::ParamValue)>, String> {
  if value.is_null() {
    return Ok(Vec::new());
  }
  let entries =
    value.as_map().ok_or_else(|| format!("Params must be an object of numbers, got {}", describe(value)))?;
  let mut out = Vec::with_capacity(entries.len());
  for (k, v) in entries {
    let param = if let Some(n) = v.as_f64() {
      alloy::ParamValue::Scalar(n as f32)
    } else if let Some(list) = v.as_list() {
      let nums: Option<Vec<f32>> = list.iter().map(|x| x.as_f64().map(|n| n as f32)).collect();
      alloy::ParamValue::Array(nums.ok_or_else(|| format!("Param '{k}' array must contain only numbers"))?)
    } else {
      return Err(format!("Param '{k}' must be a number or an array of numbers, got {}", describe(v)));
    };
    out.push((k.clone(), param));
  }
  Ok(out)
}

// { name: textureId } sampler bindings for a shader declaration, mapping
// sampler2D uniform names to texture registry ids. Null clears; a non-numeric
// entry is an error.
pub(super) fn decode_texture_bindings(value: &PropValue) -> Result<Vec<(String, u64)>, String> {
  if value.is_null() {
    return Ok(Vec::new());
  }
  let entries =
    value.as_map().ok_or_else(|| format!("Textures must be an object of texture ids, got {}", describe(value)))?;
  entries
    .iter()
    .map(|(k, t)| {
      let id = t
        .as_f64()
        .ok_or_else(|| format!("Texture binding '{k}' must be a texture id (number), got {}", describe(t)))?;
      Ok((k.clone(), id as u64))
    })
    .collect()
}

// JSX sends colors as a packed 0xRRGGBBAA u32 (parsed from a CSS string in JS).
pub(super) fn decode_color(value: &PropValue) -> Result<Color, String> {
  let rgba = value
    .as_f64()
    .ok_or_else(|| format!("Color must be a number (packed 0xRRGGBBAA), got {}", describe(value)))? as u32;
  Ok(Color::new_srgba(
    ((rgba >> 24) & 0xFF) as f32 / 255.0,
    ((rgba >> 16) & 0xFF) as f32 / 255.0,
    ((rgba >> 8) & 0xFF) as f32 / 255.0,
    (rgba & 0xFF) as f32 / 255.0,
  ))
}

// A single number applies to all four corners; an array is
// [top-left, top-right, bottom-right, bottom-left] (CSS border-radius order).
pub(super) fn decode_radius(value: &PropValue) -> Result<[f32; 4], String> {
  if let Some(arr) = value.as_list() {
    if arr.len() != 4 {
      return Err(format!(
        "Radius array must have 4 elements [top-left, top-right, bottom-right, bottom-left], got {}",
        arr.len()
      ));
    }
    Ok([
      f32_of(&arr[0], "radius[0]")?,
      f32_of(&arr[1], "radius[1]")?,
      f32_of(&arr[2], "radius[2]")?,
      f32_of(&arr[3], "radius[3]")?,
    ])
  } else {
    let v = f32_of(value, "radius")?;
    Ok([v, v, v, v])
  }
}
