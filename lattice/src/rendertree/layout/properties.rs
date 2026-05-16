use rquickjs::Value;
use taffy::geometry::Point;
use taffy::prelude::*;
use taffy::style::Overflow;

use super::util::{parse_dimension, parse_length_percentage, parse_length_percentage_auto};

pub fn set_property(style: &mut Style, property: &str, value: Value<'_>) -> Option<bool> {
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
      style.display = match value.get::<String>().expect("display must be a string").as_str() {
        "flex"  => Display::Flex,
        "block" => Display::Block,
        "grid"  => Display::Grid,
        "none"  => Display::None,
        v => panic!("unknown display value '{v}'"),
      };
    }

    // Flex container
    "flexDirection" => {
      style.flex_direction = match value.get::<String>().expect("flexDirection must be a string").as_str() {
        "row"           => FlexDirection::Row,
        "column"        => FlexDirection::Column,
        "rowReverse"    => FlexDirection::RowReverse,
        "columnReverse" => FlexDirection::ColumnReverse,
        v => panic!("unknown flexDirection value '{v}'"),
      };
    }
    "flexWrap" => {
      style.flex_wrap = match value.get::<String>().expect("flexWrap must be a string").as_str() {
        "nowrap"      => FlexWrap::NoWrap,
        "wrap"        => FlexWrap::Wrap,
        "wrapReverse" => FlexWrap::WrapReverse,
        v => panic!("unknown flexWrap value '{v}'"),
      };
    }
    "alignItems" => {
      style.align_items = Some(match value.get::<String>().expect("alignItems must be a string").as_str() {
        "start"     => AlignItems::Start,
        "end"       => AlignItems::End,
        "flexStart" => AlignItems::FlexStart,
        "flexEnd"   => AlignItems::FlexEnd,
        "center"    => AlignItems::Center,
        "baseline"  => AlignItems::Baseline,
        "stretch"   => AlignItems::Stretch,
        v => panic!("unknown alignItems value '{v}'"),
      });
    }
    "justifyContent" => {
      style.justify_content = Some(match value.get::<String>().expect("justifyContent must be a string").as_str() {
        "start"        => JustifyContent::Start,
        "end"          => JustifyContent::End,
        "flexStart"    => JustifyContent::FlexStart,
        "flexEnd"      => JustifyContent::FlexEnd,
        "center"       => JustifyContent::Center,
        "spaceBetween" => JustifyContent::SpaceBetween,
        "spaceAround"  => JustifyContent::SpaceAround,
        "spaceEvenly"  => JustifyContent::SpaceEvenly,
        "stretch"      => JustifyContent::Stretch,
        v => panic!("unknown justifyContent value '{v}'"),
      });
    }
    "alignContent" => {
      style.align_content = Some(match value.get::<String>().expect("alignContent must be a string").as_str() {
        "start"        => AlignContent::Start,
        "end"          => AlignContent::End,
        "flexStart"    => AlignContent::FlexStart,
        "flexEnd"      => AlignContent::FlexEnd,
        "center"       => AlignContent::Center,
        "spaceBetween" => AlignContent::SpaceBetween,
        "spaceAround"  => AlignContent::SpaceAround,
        "spaceEvenly"  => AlignContent::SpaceEvenly,
        "stretch"      => AlignContent::Stretch,
        v => panic!("unknown alignContent value '{v}'"),
      });
    }

    // Flex item
    "flexGrow"   => style.flex_grow   = value.get::<f64>().expect("flexGrow must be a number") as f32,
    "flexShrink" => style.flex_shrink = value.get::<f64>().expect("flexShrink must be a number") as f32,
    "flexBasis"  => style.flex_basis  = parse_dimension(value),
    "alignSelf" => {
      style.align_self = Some(match value.get::<String>().expect("alignSelf must be a string").as_str() {
        "start"     => AlignSelf::Start,
        "end"       => AlignSelf::End,
        "flexStart" => AlignSelf::FlexStart,
        "flexEnd"   => AlignSelf::FlexEnd,
        "center"    => AlignSelf::Center,
        "baseline"  => AlignSelf::Baseline,
        "stretch"   => AlignSelf::Stretch,
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
      style.position = match value.get::<String>().expect("position must be a string").as_str() {
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
    "overflow" => {
      let o = match value.get::<String>().expect("overflow must be a string").as_str() {
        "visible" => Overflow::Visible,
        "hidden"  => Overflow::Hidden,
        "scroll"  => Overflow::Scroll,
        "clip"    => Overflow::Clip,
        v => panic!("unknown overflow value '{v}'"),
      };
      style.overflow = Point { x: o, y: o };
    }

    _ => return None,
  }
  Some(true)
}
