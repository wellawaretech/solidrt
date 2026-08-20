use std::time::Duration;

use crate::audio::{clamp_rate, pan_gains, pcm_f32_all_finite, ramp_value};

const EPS: f32 = 1e-6;

#[test]
fn pan_center_is_equal_power() {
  let g = pan_gains(0.0);
  let half = std::f32::consts::FRAC_1_SQRT_2;
  assert!((g.left - half).abs() < EPS);
  assert!((g.right - half).abs() < EPS);
}

#[test]
fn pan_extremes_are_single_channel() {
  let left = pan_gains(-1.0);
  assert!((left.left - 1.0).abs() < EPS);
  assert!(left.right.abs() < EPS);
  let right = pan_gains(1.0);
  assert!(right.left.abs() < EPS);
  assert!((right.right - 1.0).abs() < EPS);
}

#[test]
fn pan_power_is_constant() {
  for i in 0..=20 {
    let g = pan_gains(-1.0 + i as f32 * 0.1);
    let power = g.left * g.left + g.right * g.right;
    assert!((power - 1.0).abs() < 1e-5, "power {power} at step {i}");
  }
}

#[test]
fn pan_out_of_range_clamps() {
  assert_eq!(pan_gains(-5.0), pan_gains(-1.0));
  assert_eq!(pan_gains(5.0), pan_gains(1.0));
  assert_eq!(pan_gains(f32::NAN), pan_gains(0.0));
}

#[test]
fn rate_clamps_to_sdl_range() {
  assert_eq!(clamp_rate(2.5), 2.5);
  assert_eq!(clamp_rate(0.0), 0.01);
  assert_eq!(clamp_rate(1000.0), 100.0);
  assert_eq!(clamp_rate(f32::NAN), 1.0);
  assert_eq!(clamp_rate(f32::INFINITY), 100.0);
}

#[test]
fn ramp_interpolates_linearly_and_finishes() {
  let ms = Duration::from_millis;
  // Midpoint of 0 -> 1 over 200 ms.
  let (v, done) = ramp_value(0.0, 1.0, ms(100), ms(200));
  assert!((v - 0.5).abs() < 1e-3, "midpoint {v}");
  assert!(!done);
  // At and past the end: exact target, finished.
  assert_eq!(ramp_value(0.0, 1.0, ms(200), ms(200)), (1.0, true));
  assert_eq!(ramp_value(0.0, 1.0, ms(999), ms(200)), (1.0, true));
  // Zero duration is instantly done at the target.
  assert_eq!(ramp_value(0.3, 0.7, ms(0), ms(0)), (0.7, true));
  // Downward ramps work the same.
  let (v, done) = ramp_value(1.0, 0.0, ms(50), ms(200));
  assert!((v - 0.75).abs() < 1e-3, "quarter {v}");
  assert!(!done);
}

#[test]
fn pcm_finite_scan_flags_non_finite() {
  let good = [0.0f32, -1.0, 1.0, 0.5];
  let bytes: Vec<u8> = good.iter().flat_map(|s| s.to_ne_bytes()).collect();
  assert!(pcm_f32_all_finite(&bytes));
  for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
    let samples = [0.0f32, bad, 0.0];
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_ne_bytes()).collect();
    assert!(!pcm_f32_all_finite(&bytes), "{bad} should be rejected");
  }
  assert!(pcm_f32_all_finite(&[]));
}
