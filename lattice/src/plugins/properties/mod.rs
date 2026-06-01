// JSX property adapter.
//
// The binding layer between the JSX/web property protocol and the
// engine-agnostic rendertree. Every frontend convention lives here - camelCase
// names, CSS-style string enums ("row-reverse", "strokeAndFill"), font aliases
// ("mono"/"sans"), packed-u32 colors, the radius-as-array shape - and gets
// decoded into native values that are handed to rendertree setters. rendertree
// knows none of it. One file per element mirrors rendertree's own structure.
//
// apply_jsx returns whether the change requires a layout invalidation; that
// decision is owned by rendertree (each setter reports it) and just threaded
// back out here.

mod layout;
mod line;
mod oval;
mod paint;
mod path;
mod rectangle;
mod text;
mod texture;
mod view;
mod window;

use std::sync::mpsc::Sender;

use alloy::AlloyCommand;
use taffy::style::Position;

use crate::plugins::value::PropValue;
use crate::rendertree::{Element, ElementKind};

pub fn apply_jsx(el: &mut Element, name: &str, value: &PropValue, cmd_tx: &Sender<AlloyCommand>) -> bool {
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
    return true;
  }

  let handled = match &mut el.kind {
    ElementKind::Window(win) => window::apply(win, name, value, cmd_tx),
    ElementKind::View(view) => view::apply(view, name, value),
    ElementKind::Rectangle(rect) => rectangle::apply(rect, name, value),
    ElementKind::Oval(oval) => oval::apply(oval, name, value),
    ElementKind::Line(line) => line::apply(line, name, value),
    ElementKind::Path(path) => path::apply(path, name, value),
    ElementKind::Text(text) => text::apply(text, name, value),
    ElementKind::Span(span) => text::apply_span(span, name, value),
    ElementKind::Texture(tex) => texture::apply(tex, name, value),
  };
  if let Some(invalidate) = handled {
    return invalidate;
  }

  if let Some(paint) = el.kind.paint_mut() {
    if let Some(invalidate) = paint::apply(paint, name, value) {
      return invalidate;
    }
  }

  if let Some(style) = el.style_mut() {
    if let Some(invalidate) = layout::apply(style, name, value) {
      return invalidate;
    }
  }

  panic!("unknown property '{name}'")
}

// Shared value decoders, kept here so every per-element module reads the same.
pub(super) fn f32_of(value: &PropValue, what: &str) -> f32 {
  value.as_f64().unwrap_or_else(|| panic!("{what} must be a number")) as f32
}

pub(super) fn str_of<'a>(value: &'a PropValue, what: &str) -> &'a str {
  value.as_str().unwrap_or_else(|| panic!("{what} must be a string"))
}
