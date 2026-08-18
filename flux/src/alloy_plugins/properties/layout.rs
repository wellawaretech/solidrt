// Layout style is taffy's own `Style`, a stable external type, so the adapter
// decodes JSX values and assigns its fields directly (choice B) rather than
// going through rendertree setters. Coupling here is to taffy, not to
// rendertree internals. Every style property affects layout.

use taffy::geometry::Point;
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

use super::{as_pct_fraction, describe, f32_of, str_of};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;

pub fn apply(style: &mut Style, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  match name {
    // Size
    "width" => style.size.width = parse_dimension(value)?,
    "height" => style.size.height = parse_dimension(value)?,
    "minWidth" => style.min_size.width = parse_dimension(value)?,
    "minHeight" => style.min_size.height = parse_dimension(value)?,
    "maxWidth" => style.max_size.width = parse_dimension(value)?,
    "maxHeight" => style.max_size.height = parse_dimension(value)?,
    "aspectRatio" => style.aspect_ratio = Some(parse_aspect_ratio(value)?),

    // Padding
    "padding" => {
      let v = parse_length_percentage(value)?;
      style.padding = Rect { top: v, right: v, bottom: v, left: v };
    }
    "paddingTop" => style.padding.top = parse_length_percentage(value)?,
    "paddingRight" => style.padding.right = parse_length_percentage(value)?,
    "paddingBottom" => style.padding.bottom = parse_length_percentage(value)?,
    "paddingLeft" => style.padding.left = parse_length_percentage(value)?,

    // Margin
    "margin" => {
      let v = parse_length_percentage_auto(value)?;
      style.margin = Rect { top: v, right: v, bottom: v, left: v };
    }
    "marginTop" => style.margin.top = parse_length_percentage_auto(value)?,
    "marginRight" => style.margin.right = parse_length_percentage_auto(value)?,
    "marginBottom" => style.margin.bottom = parse_length_percentage_auto(value)?,
    "marginLeft" => style.margin.left = parse_length_percentage_auto(value)?,

    // Display
    "display" => {
      style.display = match str_of(value, "display")? {
        "flex" => Display::Flex,
        "block" => Display::Block,
        "grid" => Display::Grid,
        "none" => Display::None,
        v => return Err(format!("Unknown display value \"{v}\"; expected flex, block, grid or none")),
      };
    }

    // Flex container
    "flexDirection" => {
      style.flex_direction = match str_of(value, "flexDirection")? {
        "row" => FlexDirection::Row,
        "column" => FlexDirection::Column,
        "row-reverse" => FlexDirection::RowReverse,
        "column-reverse" => FlexDirection::ColumnReverse,
        v => {
          return Err(format!(
            "Unknown flexDirection value \"{v}\"; expected row, column, row-reverse or column-reverse"
          ))
        }
      };
    }
    "flexWrap" => {
      style.flex_wrap = match str_of(value, "flexWrap")? {
        "nowrap" => FlexWrap::NoWrap,
        "wrap" => FlexWrap::Wrap,
        "wrap-reverse" => FlexWrap::WrapReverse,
        v => return Err(format!("Unknown flexWrap value \"{v}\"; expected nowrap, wrap or wrap-reverse")),
      };
    }
    "alignItems" => {
      style.align_items = Some(match str_of(value, "alignItems")? {
        "start" => AlignItems::START,
        "end" => AlignItems::END,
        "flex-start" => AlignItems::FLEX_START,
        "flex-end" => AlignItems::FLEX_END,
        "center" => AlignItems::CENTER,
        "baseline" => AlignItems::BASELINE,
        "stretch" => AlignItems::STRETCH,
        v => {
          return Err(format!(
            "Unknown alignItems value \"{v}\"; expected start, end, flex-start, flex-end, center, baseline or stretch"
          ))
        }
      });
    }
    "justifyContent" => {
      style.justify_content = Some(match str_of(value, "justifyContent")? {
        "start" => JustifyContent::START,
        "end" => JustifyContent::END,
        "flex-start" => JustifyContent::FLEX_START,
        "flex-end" => JustifyContent::FLEX_END,
        "center" => JustifyContent::CENTER,
        "space-between" => JustifyContent::SPACE_BETWEEN,
        "space-around" => JustifyContent::SPACE_AROUND,
        "space-evenly" => JustifyContent::SPACE_EVENLY,
        "stretch" => JustifyContent::STRETCH,
        v => {
          return Err(format!(
            "Unknown justifyContent value \"{v}\"; expected start, end, flex-start, flex-end, center, space-between, space-around, space-evenly or stretch"
          ))
        }
      });
    }
    "alignContent" => {
      style.align_content = Some(match str_of(value, "alignContent")? {
        "start" => AlignContent::START,
        "end" => AlignContent::END,
        "flex-start" => AlignContent::FLEX_START,
        "flex-end" => AlignContent::FLEX_END,
        "center" => AlignContent::CENTER,
        "space-between" => AlignContent::SPACE_BETWEEN,
        "space-around" => AlignContent::SPACE_AROUND,
        "space-evenly" => AlignContent::SPACE_EVENLY,
        "stretch" => AlignContent::STRETCH,
        v => {
          return Err(format!(
            "Unknown alignContent value \"{v}\"; expected start, end, flex-start, flex-end, center, space-between, space-around, space-evenly or stretch"
          ))
        }
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
                style.flex_grow = parse_f32(parts[0], "flex grow")?;
                style.flex_shrink = parse_f32(parts[1], "flex shrink")?;
                style.flex_basis = Dimension::length(0.0);
              }
              3 => {
                style.flex_grow = parse_f32(parts[0], "flex grow")?;
                style.flex_shrink = parse_f32(parts[1], "flex shrink")?;
                style.flex_basis = parse_dimension_str(parts[2])?;
              }
              _ => return Err(format!("Invalid flex value \"{s}\"; expected a number, none, auto, \"grow shrink\" or \"grow shrink basis\"")),
            }
          }
        }
      } else {
        return Err(format!("flex must be a number or string, got {}", describe(value)));
      }
    }
    "flexGrow" => style.flex_grow = f32_of(value, "flexGrow")?,
    "flexShrink" => style.flex_shrink = f32_of(value, "flexShrink")?,
    "flexBasis" => style.flex_basis = parse_dimension(value)?,
    "alignSelf" => {
      style.align_self = Some(match str_of(value, "alignSelf")? {
        "start" => AlignSelf::START,
        "end" => AlignSelf::END,
        "flex-start" => AlignSelf::FLEX_START,
        "flex-end" => AlignSelf::FLEX_END,
        "center" => AlignSelf::CENTER,
        "baseline" => AlignSelf::BASELINE,
        "stretch" => AlignSelf::STRETCH,
        v => {
          return Err(format!(
            "Unknown alignSelf value \"{v}\"; expected start, end, flex-start, flex-end, center, baseline or stretch"
          ))
        }
      });
    }

    // Gap
    "gap" => {
      let v = parse_length_percentage(value)?;
      style.gap = Size { width: v, height: v };
    }
    "rowGap" => style.gap.height = parse_length_percentage(value)?,
    "columnGap" => style.gap.width = parse_length_percentage(value)?,

    // Position. `position` itself is handled in apply_jsx (it also marks a
    // positioning context on the element), so only the insets land here.
    "top" => style.inset.top = parse_length_percentage_auto(value)?,
    "right" => style.inset.right = parse_length_percentage_auto(value)?,
    "bottom" => style.inset.bottom = parse_length_percentage_auto(value)?,
    "left" => style.inset.left = parse_length_percentage_auto(value)?,

    // Overflow
    "overflow" => {
      let o = parse_overflow("overflow", value)?;
      style.overflow = Point { x: o, y: o };
    }
    "overflowX" => {
      style.overflow.x = parse_overflow("overflowX", value)?;
    }
    "overflowY" => {
      style.overflow.y = parse_overflow("overflowY", value)?;
    }

    // Grid container
    "gridAutoFlow" => {
      style.grid_auto_flow = match str_of(value, "gridAutoFlow")? {
        "row" => GridAutoFlow::Row,
        "column" => GridAutoFlow::Column,
        "row-dense" => GridAutoFlow::RowDense,
        "column-dense" => GridAutoFlow::ColumnDense,
        v => return Err(format!("Unknown gridAutoFlow value \"{v}\"; expected row, column, row-dense or column-dense")),
      };
    }
    "gridTemplateColumns" => {
      style.grid_template_columns = parse_grid_template(str_of(value, "gridTemplateColumns")?)?;
    }
    "gridTemplateRows" => {
      style.grid_template_rows = parse_grid_template(str_of(value, "gridTemplateRows")?)?;
    }
    "gridAutoColumns" => {
      let v = f32_of(value, "gridAutoColumns")?;
      style.grid_auto_columns = vec![minmax(length(v), length(v))];
    }
    "gridAutoRows" => {
      let v = f32_of(value, "gridAutoRows")?;
      style.grid_auto_rows = vec![minmax(length(v), length(v))];
    }

    // Grid item
    "gridColumnStart" => style.grid_column.start = line(f32_of(value, "gridColumnStart")? as i16),
    "gridColumnEnd" => style.grid_column.end = line(f32_of(value, "gridColumnEnd")? as i16),
    "gridRowStart" => style.grid_row.start = line(f32_of(value, "gridRowStart")? as i16),
    "gridRowEnd" => style.grid_row.end = line(f32_of(value, "gridRowEnd")? as i16),

    _ => return Ok(None),
  }
  Ok(Some(Damage::Layout))
}

fn parse_f32(s: &str, what: &str) -> Result<f32, String> {
  s.parse().map_err(|_| format!("{what} must be a number, got \"{s}\""))
}

fn parse_overflow(prop: &str, value: &PropValue) -> Result<Overflow, String> {
  Ok(match str_of(value, prop)? {
    "visible" => Overflow::Visible,
    "hidden" => Overflow::Hidden,
    "scroll" => Overflow::Scroll,
    "clip" => Overflow::Clip,
    v => return Err(format!("Unknown {prop} value \"{v}\"; expected visible, hidden, scroll or clip")),
  })
}

fn parse_dimension_str(s: &str) -> Result<Dimension, String> {
  if s == "auto" {
    Ok(Dimension::auto())
  } else if s.ends_with('%') {
    let n = parse_f32(s.trim_end_matches('%'), "Percentage")?;
    Ok(Dimension::percent(n / 100.0))
  } else {
    let n: f32 = s.parse().map_err(|_| format!("Invalid dimension \"{s}\"; expected a number, percentage or auto"))?;
    Ok(Dimension::length(n))
  }
}

// taffy interprets aspect_ratio as width / height (matching CSS). Accept a bare
// number or the CSS `"16 / 9"` ratio form.
fn parse_aspect_ratio(value: &PropValue) -> Result<f32, String> {
  if let Some(n) = value.as_f64() {
    Ok(n as f32)
  } else if let Some(s) = value.as_str() {
    let parts: Vec<&str> = s.split('/').map(|p| p.trim()).collect();
    match parts.as_slice() {
      [w, h] => Ok(parse_f32(w, "aspectRatio width")? / parse_f32(h, "aspectRatio height")?),
      [r] => parse_f32(r, "aspectRatio"),
      _ => Err(format!("Invalid aspectRatio value \"{s}\"; expected a number or \"w / h\"")),
    }
  } else {
    Err(format!("aspectRatio must be a number or string, got {}", describe(value)))
  }
}

fn parse_dimension(value: &PropValue) -> Result<Dimension, String> {
  if let Some(n) = value.as_f64() {
    Ok(Dimension::length(n as f32))
  } else if let Some(f) = as_pct_fraction(value)? {
    Ok(Dimension::percent(f))
  } else if let Some(s) = value.as_str() {
    parse_dimension_str(s)
  } else {
    Err(format!("Dimension must be a number, pct(), or string, got {}", describe(value)))
  }
}

fn parse_length_percentage(value: &PropValue) -> Result<LengthPercentage, String> {
  if let Some(n) = value.as_f64() {
    Ok(LengthPercentage::length(n as f32))
  } else if let Some(f) = as_pct_fraction(value)? {
    Ok(LengthPercentage::percent(f))
  } else if let Some(s) = value.as_str() {
    if s.ends_with('%') {
      let n = parse_f32(s.trim_end_matches('%'), "Percentage")?;
      Ok(LengthPercentage::percent(n / 100.0))
    } else {
      Err(format!("Invalid length/percentage value \"{s}\""))
    }
  } else {
    Err(format!("Length/percentage must be a number, pct(), or percentage string, got {}", describe(value)))
  }
}

fn parse_length_percentage_auto(value: &PropValue) -> Result<LengthPercentageAuto, String> {
  if let Some(n) = value.as_f64() {
    Ok(LengthPercentageAuto::length(n as f32))
  } else if let Some(f) = as_pct_fraction(value)? {
    Ok(LengthPercentageAuto::percent(f))
  } else if let Some(s) = value.as_str() {
    if s == "auto" {
      Ok(LengthPercentageAuto::auto())
    } else if s.ends_with('%') {
      let n = parse_f32(s.trim_end_matches('%'), "Percentage")?;
      Ok(LengthPercentageAuto::percent(n / 100.0))
    } else {
      Err(format!("Invalid length/percentage/auto value \"{s}\""))
    }
  } else {
    Err(format!("Length/percentage/auto must be a number, pct(), or string, got {}", describe(value)))
  }
}

fn parse_grid_template(template: &str) -> Result<Vec<GridTemplateComponent<String>>, String> {
  template
    .split_whitespace()
    .map(|part| {
      let track: TrackSizingFunction = if part == "auto" {
        minmax(auto(), auto())
      } else if let Some(s) = part.strip_suffix("fr") {
        minmax(length(0.0), fr(parse_f32(s, "Grid fr track")?))
      } else if let Some(s) = part.strip_suffix("px") {
        let v = parse_f32(s, "Grid px track")?;
        minmax(length(v), length(v))
      } else {
        let v: f32 = part.parse().map_err(|_| format!("Invalid grid track \"{part}\"; expected a number, Npx, Nfr or auto"))?;
        minmax(length(v), length(v))
      };
      Ok(GridTemplateComponent::from(track))
    })
    .collect()
}
