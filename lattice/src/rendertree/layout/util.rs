use rquickjs::Value;
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

pub fn parse_dimension(value: Value<'_>) -> Dimension {
  if let Ok(n) = value.get::<f64>() {
    Dimension::length(n as f32)
  } else if let Ok(s) = value.get::<String>() {
    if s == "auto" {
      Dimension::auto()
    } else if s.ends_with('%') {
      let n: f32 = s.trim_end_matches('%').parse().expect("percentage value must be a number");
      Dimension::percent(n / 100.0)
    } else {
      panic!("invalid dimension value: '{s}'")
    }
  } else {
    panic!("dimension must be a number or string")
  }
}

pub fn parse_length_percentage(value: Value<'_>) -> LengthPercentage {
  if let Ok(n) = value.get::<f64>() {
    LengthPercentage::length(n as f32)
  } else if let Ok(s) = value.get::<String>() {
    if s.ends_with('%') {
      let n: f32 = s.trim_end_matches('%').parse().expect("percentage value must be a number");
      LengthPercentage::percent(n / 100.0)
    } else {
      panic!("invalid length/percentage value: '{s}'")
    }
  } else {
    panic!("length/percentage must be a number or percentage string")
  }
}

pub fn parse_length_percentage_auto(value: Value<'_>) -> LengthPercentageAuto {
  if let Ok(n) = value.get::<f64>() {
    LengthPercentageAuto::length(n as f32)
  } else if let Ok(s) = value.get::<String>() {
    if s == "auto" {
      LengthPercentageAuto::auto()
    } else if s.ends_with('%') {
      let n: f32 = s.trim_end_matches('%').parse().expect("percentage value must be a number");
      LengthPercentageAuto::percent(n / 100.0)
    } else {
      panic!("invalid length/percentage/auto value: '{s}'")
    }
  } else {
    panic!("length/percentage/auto must be a number or string")
  }
}
