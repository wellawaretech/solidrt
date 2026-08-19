use crate::color::*;
use crate::impellers::Color;

#[test]
fn parse_covers_the_css_grammar() {
  let hex = parse_css("#ff8000").expect("hex parses");
  assert!((hex.red - 1.0).abs() < 1e-3 && (hex.green - 128.0 / 255.0).abs() < 1e-2 && hex.blue < 1e-3);
  let named = parse_css("tomato").expect("named parses");
  assert!(named.red > 0.9 && named.green > 0.3 && named.blue < 0.35);
  let rgba = parse_css("rgba(0, 0, 255, 0.5)").expect("rgba parses");
  assert!((rgba.blue - 1.0).abs() < 1e-3 && (rgba.alpha - 0.5).abs() < 1e-2);
  parse_css("hsl(120, 100%, 50%)").expect("hsl parses");
  assert!(parse_css("no-such-color").is_err());
  assert!(parse_css("").is_err());
}

#[test]
fn mix_endpoints_are_exact_and_midpoint_is_saturated() {
  let red = Color::new_srgba(1.0, 0.0, 0.0, 1.0);
  let blue = Color::new_srgba(0.0, 0.0, 1.0, 1.0);
  let at0 = mix(red, blue, 0.0);
  let at1 = mix(red, blue, 1.0);
  assert!((at0.red - 1.0).abs() < 1e-3 && at0.blue < 1e-3);
  assert!((at1.blue - 1.0).abs() < 1e-3 && at1.red < 1e-3);
  // The oklab midpoint keeps both endpoints present instead of collapsing
  // toward the 0.5/0.5 gray an sRGB lerp gives.
  let mid = mix(red, blue, 0.5);
  assert!(mid.red > 0.05 && mid.blue > 0.05, "midpoint carries both: {mid:?}");
}

#[test]
fn brightness_matches_the_yiq_poles() {
  assert!(brightness(Color::new_srgba(0.0, 0.0, 0.0, 1.0)) < 1e-3);
  assert!((brightness(Color::new_srgba(1.0, 1.0, 1.0, 1.0)) - 1.0).abs() < 1e-3);
  // Green reads brighter than blue at equal channel value (the YIQ point).
  let g = brightness(Color::new_srgba(0.0, 1.0, 0.0, 1.0));
  let b = brightness(Color::new_srgba(0.0, 0.0, 1.0, 1.0));
  assert!(g > b);
}
