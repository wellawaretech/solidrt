// Layout style is taffy's own `Style`, a stable external type, so the adapter
// decodes JSX values and assigns its fields directly (choice B) rather than
// going through rendertree setters. Coupling here is to taffy, not to
// rendertree internals. Every style property affects layout.

use taffy::geometry::Point;
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

use super::{as_pct_fraction, f32_of, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;

pub fn apply(style: &mut Style, name: &str, value: &PropValue) -> Option<Damage> {
  match name {
    // Size
    "width" => style.size.width = parse_dimension(value),
    "height" => style.size.height = parse_dimension(value),
    "minWidth" => style.min_size.width = parse_dimension(value),
    "minHeight" => style.min_size.height = parse_dimension(value),
    "maxWidth" => style.max_size.width = parse_dimension(value),
    "maxHeight" => style.max_size.height = parse_dimension(value),
    "aspectRatio" => style.aspect_ratio = Some(parse_aspect_ratio(value)),

    // Padding
    "padding" => {
      let v = parse_length_percentage(value);
      style.padding = Rect { top: v, right: v, bottom: v, left: v };
    }
    "paddingTop" => style.padding.top = parse_length_percentage(value),
    "paddingRight" => style.padding.right = parse_length_percentage(value),
    "paddingBottom" => style.padding.bottom = parse_length_percentage(value),
    "paddingLeft" => style.padding.left = parse_length_percentage(value),

    // Margin
    "margin" => {
      let v = parse_length_percentage_auto(value);
      style.margin = Rect { top: v, right: v, bottom: v, left: v };
    }
    "marginTop" => style.margin.top = parse_length_percentage_auto(value),
    "marginRight" => style.margin.right = parse_length_percentage_auto(value),
    "marginBottom" => style.margin.bottom = parse_length_percentage_auto(value),
    "marginLeft" => style.margin.left = parse_length_percentage_auto(value),

    // Display
    "display" => {
      style.display = match str_of(value, "display") {
        "flex" => Display::Flex,
        "block" => Display::Block,
        "grid" => Display::Grid,
        "none" => Display::None,
        v => panic!("unknown display value '{v}'"),
      };
    }

    // Flex container
    "flexDirection" => {
      style.flex_direction = match str_of(value, "flexDirection") {
        "row" => FlexDirection::Row,
        "column" => FlexDirection::Column,
        "row-reverse" => FlexDirection::RowReverse,
        "column-reverse" => FlexDirection::ColumnReverse,
        v => panic!("unknown flexDirection value '{v}'"),
      };
    }
    "flexWrap" => {
      style.flex_wrap = match str_of(value, "flexWrap") {
        "nowrap" => FlexWrap::NoWrap,
        "wrap" => FlexWrap::Wrap,
        "wrap-reverse" => FlexWrap::WrapReverse,
        v => panic!("unknown flexWrap value '{v}'"),
      };
    }
    "alignItems" => {
      style.align_items = Some(match str_of(value, "alignItems") {
        "start" => AlignItems::START,
        "end" => AlignItems::END,
        "flex-start" => AlignItems::FLEX_START,
        "flex-end" => AlignItems::FLEX_END,
        "center" => AlignItems::CENTER,
        "baseline" => AlignItems::BASELINE,
        "stretch" => AlignItems::STRETCH,
        v => panic!("unknown alignItems value '{v}'"),
      });
    }
    "justifyContent" => {
      style.justify_content = Some(match str_of(value, "justifyContent") {
        "start" => JustifyContent::START,
        "end" => JustifyContent::END,
        "flex-start" => JustifyContent::FLEX_START,
        "flex-end" => JustifyContent::FLEX_END,
        "center" => JustifyContent::CENTER,
        "space-between" => JustifyContent::SPACE_BETWEEN,
        "space-around" => JustifyContent::SPACE_AROUND,
        "space-evenly" => JustifyContent::SPACE_EVENLY,
        "stretch" => JustifyContent::STRETCH,
        v => panic!("unknown justifyContent value '{v}'"),
      });
    }
    "alignContent" => {
      style.align_content = Some(match str_of(value, "alignContent") {
        "start" => AlignContent::START,
        "end" => AlignContent::END,
        "flex-start" => AlignContent::FLEX_START,
        "flex-end" => AlignContent::FLEX_END,
        "center" => AlignContent::CENTER,
        "space-between" => AlignContent::SPACE_BETWEEN,
        "space-around" => AlignContent::SPACE_AROUND,
        "space-evenly" => AlignContent::SPACE_EVENLY,
        "stretch" => AlignContent::STRETCH,
        v => panic!("unknown alignContent value '{v}'"),
      });
    }

    // Flex item
    "flex" => {
      if let Some(n) = value.as_f64() {
        style.flex_grow = n as f32;
        style.flex_shrink = 1.0;
        style.flex_basis = Dimension::length(0.0);
      } else if let Some(s) = value.as_str() {
        match s {
          "none" => {
            style.flex_grow = 0.0;
            style.flex_shrink = 0.0;
            style.flex_basis = Dimension::auto();
          }
          "auto" => {
            style.flex_grow = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis = Dimension::auto();
          }
          s => {
            let parts: Vec<&str> = s.split_whitespace().collect();
            match parts.len() {
              2 => {
                style.flex_grow = parts[0].parse().expect("flex grow must be a number");
                style.flex_shrink = parts[1].parse().expect("flex shrink must be a number");
                style.flex_basis = Dimension::length(0.0);
              }
              3 => {
                style.flex_grow = parts[0].parse().expect("flex grow must be a number");
                style.flex_shrink = parts[1].parse().expect("flex shrink must be a number");
                style.flex_basis = parse_dimension_str(parts[2]);
              }
              _ => panic!("invalid flex value: '{s}'"),
            }
          }
        }
      } else {
        panic!("flex must be a number or string")
      }
    }
    "flexGrow" => style.flex_grow = f32_of(value, "flexGrow"),
    "flexShrink" => style.flex_shrink = f32_of(value, "flexShrink"),
    "flexBasis" => style.flex_basis = parse_dimension(value),
    "alignSelf" => {
      style.align_self = Some(match str_of(value, "alignSelf") {
        "start" => AlignSelf::START,
        "end" => AlignSelf::END,
        "flex-start" => AlignSelf::FLEX_START,
        "flex-end" => AlignSelf::FLEX_END,
        "center" => AlignSelf::CENTER,
        "baseline" => AlignSelf::BASELINE,
        "stretch" => AlignSelf::STRETCH,
        v => panic!("unknown alignSelf value '{v}'"),
      });
    }

    // Gap
    "gap" => {
      let v = parse_length_percentage(value);
      style.gap = Size { width: v, height: v };
    }
    "rowGap" => style.gap.height = parse_length_percentage(value),
    "columnGap" => style.gap.width = parse_length_percentage(value),

    // Position. `position` itself is handled in apply_jsx (it also marks a
    // positioning context on the element), so only the insets land here.
    "top" => style.inset.top = parse_length_percentage_auto(value),
    "right" => style.inset.right = parse_length_percentage_auto(value),
    "bottom" => style.inset.bottom = parse_length_percentage_auto(value),
    "left" => style.inset.left = parse_length_percentage_auto(value),

    // Overflow
    "overflow" => {
      let o = parse_overflow("overflow", value);
      style.overflow = Point { x: o, y: o };
    }
    "overflowX" => {
      style.overflow.x = parse_overflow("overflowX", value);
    }
    "overflowY" => {
      style.overflow.y = parse_overflow("overflowY", value);
    }

    // Grid container
    "gridAutoFlow" => {
      style.grid_auto_flow = match str_of(value, "gridAutoFlow") {
        "row" => GridAutoFlow::Row,
        "column" => GridAutoFlow::Column,
        "row-dense" => GridAutoFlow::RowDense,
        "column-dense" => GridAutoFlow::ColumnDense,
        v => panic!("unknown gridAutoFlow value '{v}'"),
      };
    }
    "gridTemplateColumns" => {
      style.grid_template_columns = parse_grid_template(str_of(value, "gridTemplateColumns"));
    }
    "gridTemplateRows" => {
      style.grid_template_rows = parse_grid_template(str_of(value, "gridTemplateRows"));
    }
    "gridAutoColumns" => {
      let v = f32_of(value, "gridAutoColumns");
      style.grid_auto_columns = vec![minmax(length(v), length(v))];
    }
    "gridAutoRows" => {
      let v = f32_of(value, "gridAutoRows");
      style.grid_auto_rows = vec![minmax(length(v), length(v))];
    }

    // Grid item
    "gridColumnStart" => {
      style.grid_column.start = line(value.as_f64().expect("gridColumnStart must be a number") as i16)
    }
    "gridColumnEnd" => style.grid_column.end = line(value.as_f64().expect("gridColumnEnd must be a number") as i16),
    "gridRowStart" => style.grid_row.start = line(value.as_f64().expect("gridRowStart must be a number") as i16),
    "gridRowEnd" => style.grid_row.end = line(value.as_f64().expect("gridRowEnd must be a number") as i16),

    _ => return None,
  }
  Some(Damage::Layout)
}

fn parse_overflow(prop: &str, value: &PropValue) -> Overflow {
  match str_of(value, prop) {
    "visible" => Overflow::Visible,
    "hidden" => Overflow::Hidden,
    "scroll" => Overflow::Scroll,
    "clip" => Overflow::Clip,
    v => panic!("unknown {prop} value '{v}'"),
  }
}

fn parse_dimension_str(s: &str) -> Dimension {
  if s == "auto" {
    Dimension::auto()
  } else if s.ends_with('%') {
    let n: f32 = s.trim_end_matches('%').parse().expect("percentage value must be a number");
    Dimension::percent(n / 100.0)
  } else {
    let n: f32 = s.parse().expect("dimension value must be a number or 'auto'");
    Dimension::length(n)
  }
}

// taffy interprets aspect_ratio as width / height (matching CSS). Accept a bare
// number or the CSS `"16 / 9"` ratio form.
fn parse_aspect_ratio(value: &PropValue) -> f32 {
  if let Some(n) = value.as_f64() {
    n as f32
  } else if let Some(s) = value.as_str() {
    let parts: Vec<&str> = s.split('/').map(|p| p.trim()).collect();
    match parts.as_slice() {
      [w, h] => {
        let w: f32 = w.parse().expect("aspectRatio width must be a number");
        let h: f32 = h.parse().expect("aspectRatio height must be a number");
        w / h
      }
      [r] => r.parse().expect("aspectRatio must be a number"),
      _ => panic!("invalid aspectRatio value: '{s}'"),
    }
  } else {
    panic!("aspectRatio must be a number or string")
  }
}

fn parse_dimension(value: &PropValue) -> Dimension {
  if let Some(n) = value.as_f64() {
    Dimension::length(n as f32)
  } else if let Some(f) = as_pct_fraction(value) {
    Dimension::percent(f)
  } else if let Some(s) = value.as_str() {
    parse_dimension_str(s)
  } else {
    panic!("dimension must be a number, pct(), or string")
  }
}

fn parse_length_percentage(value: &PropValue) -> LengthPercentage {
  if let Some(n) = value.as_f64() {
    LengthPercentage::length(n as f32)
  } else if let Some(f) = as_pct_fraction(value) {
    LengthPercentage::percent(f)
  } else if let Some(s) = value.as_str() {
    if s.ends_with('%') {
      let n: f32 = s.trim_end_matches('%').parse().expect("percentage value must be a number");
      LengthPercentage::percent(n / 100.0)
    } else {
      panic!("invalid length/percentage value: '{s}'")
    }
  } else {
    panic!("length/percentage must be a number, pct(), or percentage string")
  }
}

fn parse_length_percentage_auto(value: &PropValue) -> LengthPercentageAuto {
  if let Some(n) = value.as_f64() {
    LengthPercentageAuto::length(n as f32)
  } else if let Some(f) = as_pct_fraction(value) {
    LengthPercentageAuto::percent(f)
  } else if let Some(s) = value.as_str() {
    if s == "auto" {
      LengthPercentageAuto::auto()
    } else if s.ends_with('%') {
      let n: f32 = s.trim_end_matches('%').parse().expect("percentage value must be a number");
      LengthPercentageAuto::percent(n / 100.0)
    } else {
      panic!("invalid length/percentage/auto value: '{s}'")
    }
  } else {
    panic!("length/percentage/auto must be a number, pct(), or string")
  }
}

fn parse_grid_template(template: &str) -> Vec<GridTemplateComponent<String>> {
  template
    .split_whitespace()
    .map(|part| {
      let track: TrackSizingFunction = if part == "auto" {
        minmax(auto(), auto())
      } else if let Some(s) = part.strip_suffix("fr") {
        let v: f32 = s.parse().expect("fr value must be a number");
        minmax(length(0.0), fr(v))
      } else if let Some(s) = part.strip_suffix("px") {
        let v: f32 = s.parse().expect("px value must be a number");
        minmax(length(v), length(v))
      } else {
        let v: f32 = part.parse().expect("grid track value must be a number");
        minmax(length(v), length(v))
      };
      GridTemplateComponent::from(track)
    })
    .collect()
}
