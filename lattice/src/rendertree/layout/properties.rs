use taffy::geometry::Point;
use taffy::prelude::*;
use taffy::style::Overflow;

use super::util::{parse_dimension, parse_dimension_str, parse_grid_template, parse_length_percentage, parse_length_percentage_auto};
use crate::rendertree::PropValue;

pub fn set_property(style: &mut Style, property: &str, value: &PropValue) -> Option<bool> {
  match property {
    // Size
    "width"     => style.size.width      = parse_dimension(value),
    "height"    => style.size.height     = parse_dimension(value),
    "minWidth"  => style.min_size.width  = parse_dimension(value),
    "minHeight" => style.min_size.height = parse_dimension(value),
    "maxWidth"  => style.max_size.width  = parse_dimension(value),
    "maxHeight" => style.max_size.height = parse_dimension(value),

    // Padding
    "padding" => {
      let v = parse_length_percentage(value);
      style.padding = Rect { top: v, right: v, bottom: v, left: v };
    }
    "paddingTop"    => style.padding.top    = parse_length_percentage(value),
    "paddingRight"  => style.padding.right  = parse_length_percentage(value),
    "paddingBottom" => style.padding.bottom = parse_length_percentage(value),
    "paddingLeft"   => style.padding.left   = parse_length_percentage(value),

    // Margin
    "margin" => {
      let v = parse_length_percentage_auto(value);
      style.margin = Rect { top: v, right: v, bottom: v, left: v };
    }
    "marginTop"    => style.margin.top    = parse_length_percentage_auto(value),
    "marginRight"  => style.margin.right  = parse_length_percentage_auto(value),
    "marginBottom" => style.margin.bottom = parse_length_percentage_auto(value),
    "marginLeft"   => style.margin.left   = parse_length_percentage_auto(value),

    // Display
    "display" => {
      style.display = match value.as_str().expect("display must be a string") {
        "flex"  => Display::Flex,
        "block" => Display::Block,
        "grid"  => Display::Grid,
        "none"  => Display::None,
        v => panic!("unknown display value '{v}'"),
      };
    }

    // Flex container
    "flexDirection" => {
      style.flex_direction = match value.as_str().expect("flexDirection must be a string") {
        "row"            => FlexDirection::Row,
        "column"         => FlexDirection::Column,
        "row-reverse"    => FlexDirection::RowReverse,
        "column-reverse" => FlexDirection::ColumnReverse,
        v => panic!("unknown flexDirection value '{v}'"),
      };
    }
    "flexWrap" => {
      style.flex_wrap = match value.as_str().expect("flexWrap must be a string") {
        "nowrap"       => FlexWrap::NoWrap,
        "wrap"         => FlexWrap::Wrap,
        "wrap-reverse" => FlexWrap::WrapReverse,
        v => panic!("unknown flexWrap value '{v}'"),
      };
    }
    "alignItems" => {
      style.align_items = Some(match value.as_str().expect("alignItems must be a string") {
        "start"      => AlignItems::Start,
        "end"        => AlignItems::End,
        "flex-start" => AlignItems::FlexStart,
        "flex-end"   => AlignItems::FlexEnd,
        "center"     => AlignItems::Center,
        "baseline"   => AlignItems::Baseline,
        "stretch"    => AlignItems::Stretch,
        v => panic!("unknown alignItems value '{v}'"),
      });
    }
    "justifyContent" => {
      style.justify_content = Some(match value.as_str().expect("justifyContent must be a string") {
        "start"         => JustifyContent::Start,
        "end"           => JustifyContent::End,
        "flex-start"    => JustifyContent::FlexStart,
        "flex-end"      => JustifyContent::FlexEnd,
        "center"        => JustifyContent::Center,
        "space-between" => JustifyContent::SpaceBetween,
        "space-around"  => JustifyContent::SpaceAround,
        "space-evenly"  => JustifyContent::SpaceEvenly,
        "stretch"       => JustifyContent::Stretch,
        v => panic!("unknown justifyContent value '{v}'"),
      });
    }
    "alignContent" => {
      style.align_content = Some(match value.as_str().expect("alignContent must be a string") {
        "start"         => AlignContent::Start,
        "end"           => AlignContent::End,
        "flex-start"    => AlignContent::FlexStart,
        "flex-end"      => AlignContent::FlexEnd,
        "center"        => AlignContent::Center,
        "space-between" => AlignContent::SpaceBetween,
        "space-around"  => AlignContent::SpaceAround,
        "space-evenly"  => AlignContent::SpaceEvenly,
        "stretch"       => AlignContent::Stretch,
        v => panic!("unknown alignContent value '{v}'"),
      });
    }

    // Flex item
    "flex" => {
      if let Some(n) = value.as_f64() {
        style.flex_grow   = n as f32;
        style.flex_shrink = 1.0;
        style.flex_basis  = Dimension::length(0.0);
      } else if let Some(s) = value.as_str() {
        match s {
          "none" => {
            style.flex_grow   = 0.0;
            style.flex_shrink = 0.0;
            style.flex_basis  = Dimension::auto();
          }
          "auto" => {
            style.flex_grow   = 1.0;
            style.flex_shrink = 1.0;
            style.flex_basis  = Dimension::auto();
          }
          s => {
            let parts: Vec<&str> = s.split_whitespace().collect();
            match parts.len() {
              2 => {
                style.flex_grow   = parts[0].parse().expect("flex grow must be a number");
                style.flex_shrink = parts[1].parse().expect("flex shrink must be a number");
                style.flex_basis  = Dimension::length(0.0);
              }
              3 => {
                style.flex_grow   = parts[0].parse().expect("flex grow must be a number");
                style.flex_shrink = parts[1].parse().expect("flex shrink must be a number");
                style.flex_basis  = parse_dimension_str(parts[2]);
              }
              _ => panic!("invalid flex value: '{s}'"),
            }
          }
        }
      } else {
        panic!("flex must be a number or string")
      }
    }
    "flexGrow"   => style.flex_grow   = value.as_f64().expect("flexGrow must be a number") as f32,
    "flexShrink" => style.flex_shrink = value.as_f64().expect("flexShrink must be a number") as f32,
    "flexBasis"  => style.flex_basis  = parse_dimension(value),
    "alignSelf" => {
      style.align_self = Some(match value.as_str().expect("alignSelf must be a string") {
        "start"      => AlignSelf::Start,
        "end"        => AlignSelf::End,
        "flex-start" => AlignSelf::FlexStart,
        "flex-end"   => AlignSelf::FlexEnd,
        "center"     => AlignSelf::Center,
        "baseline"   => AlignSelf::Baseline,
        "stretch"    => AlignSelf::Stretch,
        v => panic!("unknown alignSelf value '{v}'"),
      });
    }

    // Gap
    "gap" => {
      let v = parse_length_percentage(value);
      style.gap = Size { width: v, height: v };
    }
    "rowGap"    => style.gap.height = parse_length_percentage(value),
    "columnGap" => style.gap.width  = parse_length_percentage(value),

    // Position
    "position" => {
      style.position = match value.as_str().expect("position must be a string") {
        "relative" => Position::Relative,
        "absolute" => Position::Absolute,
        v => panic!("unknown position value '{v}'"),
      };
    }
    "top"    => style.inset.top    = parse_length_percentage_auto(value),
    "right"  => style.inset.right  = parse_length_percentage_auto(value),
    "bottom" => style.inset.bottom = parse_length_percentage_auto(value),
    "left"   => style.inset.left   = parse_length_percentage_auto(value),

    // Overflow
    "overflow"  => { let o = parse_overflow("overflow",  value); style.overflow = Point { x: o, y: o }; }
    "overflowX" => { style.overflow.x = parse_overflow("overflowX", value); }
    "overflowY" => { style.overflow.y = parse_overflow("overflowY", value); }

    // Grid container
    "gridAutoFlow" => {
      style.grid_auto_flow = match value.as_str().expect("gridAutoFlow must be a string") {
        "row"           => GridAutoFlow::Row,
        "column"        => GridAutoFlow::Column,
        "row-dense"     => GridAutoFlow::RowDense,
        "column-dense"  => GridAutoFlow::ColumnDense,
        v => panic!("unknown gridAutoFlow value '{v}'"),
      };
    }
    "gridTemplateColumns" => {
      style.grid_template_columns = parse_grid_template(value.as_str().expect("gridTemplateColumns must be a string"));
    }
    "gridTemplateRows" => {
      style.grid_template_rows = parse_grid_template(value.as_str().expect("gridTemplateRows must be a string"));
    }
    "gridAutoColumns" => {
      let v = value.as_f64().expect("gridAutoColumns must be a number") as f32;
      style.grid_auto_columns = vec![minmax(length(v), length(v))];
    }
    "gridAutoRows" => {
      let v = value.as_f64().expect("gridAutoRows must be a number") as f32;
      style.grid_auto_rows = vec![minmax(length(v), length(v))];
    }

    // Grid item
    "gridColumnStart" => style.grid_column.start = line(value.as_f64().expect("gridColumnStart must be a number") as i16),
    "gridColumnEnd"   => style.grid_column.end   = line(value.as_f64().expect("gridColumnEnd must be a number") as i16),
    "gridRowStart"    => style.grid_row.start     = line(value.as_f64().expect("gridRowStart must be a number") as i16),
    "gridRowEnd"      => style.grid_row.end       = line(value.as_f64().expect("gridRowEnd must be a number") as i16),

    _ => return None,
  }
  Some(true)
}

fn parse_overflow(prop: &str, value: &PropValue) -> Overflow {
  let s = value.as_str().unwrap_or_else(|| panic!("{prop} must be a string"));
  match s {
    "visible" => Overflow::Visible,
    "hidden"  => Overflow::Hidden,
    "scroll"  => Overflow::Scroll,
    "clip"    => Overflow::Clip,
    v => panic!("unknown {prop} value '{v}'"),
  }
}
