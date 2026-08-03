use crate::audio::pan_gains;

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
