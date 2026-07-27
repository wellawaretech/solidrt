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
// threaded back out here for the caller to hand to RenderTree::apply_damage.

mod layout;
mod line;
mod oval;
mod paint;
mod path;
mod rectangle;
mod svg;
mod text;
mod texture;
mod view;
mod window;

use std::sync::mpsc::Sender;

use alloy::AlloyCommand;
use taffy::style::Position;

use crate::plugins::gui::value::PropValue;
use alloy::impellers::Color;
use alloy::rendertree::{BoundaryMode, Damage, Element, ElementKind, PointerEvents};

// Returns Ok(damage) on success; Err(message) for an unknown property, which
// the FFI caller surfaces as a throwable JS error rather than aborting the
// process. A single typo'd or unsupported prop must not take down the runtime.
pub fn apply_jsx(
  el: &mut Element,
  name: &str,
  value: &PropValue,
  cmd_tx: &Sender<AlloyCommand>,
) -> Result<Damage, String> {
  // `position` is decoded here rather than in the layout style adapter because
  // it has a side effect beyond the taffy Style: it marks the element as a
  // positioning context used to resolve container-relative bounding boxes.
  if name == "position" {
    let position = match str_of(value, "position") {
      "relative" => Position::Relative,
      "absolute" => Position::Absolute,
      v => panic!("unknown position value '{v}'"),
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
      _ => panic!("repaintBoundary must be a boolean, \"snapshot\", or \"snapshot-no-aa\""),
    };
    return Ok(Damage::Paint);
  }

  // Element-level, kind-independent: controls hit testing (see hit.rs). Paint/hit
  // only, no layout invalidation. `pointerEvents` is inherited (like CSS): a
  // null clears any local override rather than forcing Auto, so components
  // that forward this prop even when the app never set it do not break
  // inheritance from an ancestor that did.
  if name == "pointerEvents" {
    let pointer_events = match value {
      PropValue::Null => None,
      _ => match str_of(value, "pointerEvents") {
        "auto" => Some(PointerEvents::Auto),
        "none" => Some(PointerEvents::None),
        "all" => Some(PointerEvents::All),
        v => panic!("unknown pointerEvents value '{v}'"),
      },
    };
    el.set_pointer_events(pointer_events);
    return Ok(Damage::None);
  }

  let handled = match &mut el.kind {
    ElementKind::Window(win) => window::apply(win, name, value, cmd_tx),
    ElementKind::View(view) => view::apply(view, name, value),
    ElementKind::Rectangle(rect) => rectangle::apply(rect, name, value),
    ElementKind::Oval(oval) => oval::apply(oval, name, value),
    ElementKind::Line(line) => line::apply(line, name, value),
    ElementKind::Path(path) => path::apply(path, name, value),
    ElementKind::Svg(svg) => svg::apply(svg, name, value),
    ElementKind::Text(text) => text::apply(text, name, value),
    ElementKind::Span(span) => text::apply_span(span, name, value),
    ElementKind::Texture(tex) => texture::apply(tex, name, value),
  };
  if let Some(damage) = handled {
    return Ok(damage);
  }

  if let Some(paint) = el.kind.paint_mut() {
    if let Some(damage) = paint::apply(paint, name, value) {
      return Ok(damage);
    }
  }

  if let Some(style) = el.style_mut() {
    if let Some(damage) = layout::apply(style, name, value) {
      return Ok(damage);
    }
  }

  Err(format!("unknown property '{name}'"))
}

// Shared value decoders, kept here so every per-element module reads the same.
pub(super) fn f32_of(value: &PropValue, what: &str) -> f32 {
  value.as_f64().unwrap_or_else(|| panic!("{what} must be a number")) as f32
}

// The branded `pct(n)` value from JS: { __unit: "pct", v: n }. Returns the
// fraction (n / 100), or None for any value that is not a pct. Lets a
// percentage cross the boundary as a first-class value that no consumer has to
// string-parse; a bare number stays pixels.
pub(super) fn as_pct_fraction(value: &PropValue) -> Option<f32> {
  if value.get("__unit").and_then(PropValue::as_str) == Some("pct") {
    let v = value.get("v").and_then(PropValue::as_f64).expect("pct value must be a number") as f32;
    Some(v / 100.0)
  } else {
    None
  }
}

pub(super) fn str_of<'a>(value: &'a PropValue, what: &str) -> &'a str {
  value.as_str().unwrap_or_else(|| panic!("{what} must be a string"))
}

// JSX sends colors as a packed 0xRRGGBBAA u32 (parsed from a CSS string in JS).
pub(super) fn decode_color(value: &PropValue) -> Color {
  let rgba = value.as_f64().expect("color must be a number") as u32;
  Color::new_srgba(
    ((rgba >> 24) & 0xFF) as f32 / 255.0,
    ((rgba >> 16) & 0xFF) as f32 / 255.0,
    ((rgba >> 8) & 0xFF) as f32 / 255.0,
    (rgba & 0xFF) as f32 / 255.0,
  )
}

// A single number applies to all four corners; an array is
// [top-left, top-right, bottom-right, bottom-left] (CSS border-radius order).
pub(super) fn decode_radius(value: &PropValue) -> [f32; 4] {
  if let Some(arr) = value.as_list() {
    if arr.len() != 4 {
      panic!("radius array must have 4 elements [top-left, top-right, bottom-right, bottom-left]");
    }
    [
      arr[0].as_f64().expect("radius[0] must be a number") as f32,
      arr[1].as_f64().expect("radius[1] must be a number") as f32,
      arr[2].as_f64().expect("radius[2] must be a number") as f32,
      arr[3].as_f64().expect("radius[3] must be a number") as f32,
    ]
  } else {
    let v = value.as_f64().expect("radius must be a number or an array of 4 numbers") as f32;
    [v, v, v, v]
  }
}
