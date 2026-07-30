use std::collections::HashMap;

use crate::gpu::{validate_draw_bound, validate_params, validate_texture_bindings, ParamValue, UniformKind, UniformTable};

fn table(entries: &[(&str, UniformKind)]) -> UniformTable {
  entries.iter().map(|(name, kind)| (name.to_string(), *kind)).collect()
}

fn scalar(name: &str, v: f32) -> (String, ParamValue) {
  (name.to_string(), ParamValue::Scalar(v))
}

fn array(name: &str, n: usize) -> (String, ParamValue) {
  (name.to_string(), ParamValue::Array(vec![0.0; n]))
}

#[test]
fn params_matching_kinds_pass() {
  let t = table(&[
    ("uTime", UniformKind::Float),
    ("uCount", UniformKind::Int),
    ("uOn", UniformKind::Bool),
    ("uPos", UniformKind::Vec2),
    ("uColor", UniformKind::Vec4),
    ("uModel", UniformKind::Mat4),
  ]);
  let params = [
    scalar("uTime", 1.5),
    scalar("uCount", 3.0),
    array("uOn", 1),
    array("uPos", 2),
    array("uColor", 4),
    array("uModel", 16),
  ];
  assert_eq!(validate_params(&t, &params), Ok(()));
}

#[test]
fn params_unknown_name_errors_and_lists_active() {
  let t = table(&[("uColor", UniformKind::Vec4), ("uTime", UniformKind::Float)]);
  let err = validate_params(&t, &[scalar("uColr", 1.0)]).expect_err("typo must error");
  assert!(err.contains("no active uniform named 'uColr'"), "{err}");
  // The active list is sorted, so the message is deterministic.
  assert!(err.contains("uColor, uTime"), "{err}");
}

#[test]
fn params_unknown_name_on_empty_table_errors() {
  let err = validate_params(&HashMap::new(), &[scalar("uAnything", 0.0)]).expect_err("must error");
  assert!(err.contains("the program has none"), "{err}");
}

#[test]
fn params_component_mismatch_errors() {
  let t = table(&[("uPos", UniformKind::Vec3)]);
  let err = validate_params(&t, &[array("uPos", 2)]).expect_err("arity mismatch must error");
  assert!(err.contains("uPos") && err.contains("vec3") && err.contains("2 component"), "{err}");
  let err = validate_params(&t, &[scalar("uPos", 1.0)]).expect_err("scalar for vec3 must error");
  assert!(err.contains("expects 3"), "{err}");
}

#[test]
fn params_sampler_and_unsupported_kinds_error() {
  let t = table(&[("uTex", UniformKind::Sampler2D), ("uIvec", UniformKind::Other)]);
  let err = validate_params(&t, &[scalar("uTex", 1.0)]).expect_err("sampler via params must error");
  assert!(err.contains("bind it via textures"), "{err}");
  let err = validate_params(&t, &[scalar("uIvec", 1.0)]).expect_err("unsupported kind must error");
  assert!(err.contains("unsupported uniform type"), "{err}");
}

#[test]
fn texture_bindings_require_active_sampler2d() {
  let t = table(&[("uTex", UniformKind::Sampler2D), ("uColor", UniformKind::Vec4)]);
  assert_eq!(validate_texture_bindings(&t, &[("uTex".to_string(), 7)]), Ok(()));
  let err = validate_texture_bindings(&t, &[("uColor".to_string(), 7)]).expect_err("non-sampler must error");
  assert!(err.contains("uColor") && err.contains("not a sampler2D"), "{err}");
  let err = validate_texture_bindings(&t, &[("uTx".to_string(), 7)]).expect_err("typo must error");
  assert!(err.contains("no active uniform named 'uTx'"), "{err}");
}

#[test]
fn draw_bound_within_buffer_passes() {
  // 100 vertices at 20 bytes each in a 2000-byte buffer: exactly full.
  assert_eq!(validate_draw_bound(100, 20, 2000), Ok(()));
  assert_eq!(validate_draw_bound(0, 20, 2000), Ok(()));
  // Attributeless callers pass stride 0 / size 0: any non-negative count.
  assert_eq!(validate_draw_bound(1_000_000, 0, 0), Ok(()));
}

#[test]
fn draw_bound_past_buffer_end_errors() {
  let err = validate_draw_bound(101, 20, 2000).expect_err("one vertex past the end must error");
  assert!(err.contains("101") && err.contains("2020 bytes") && err.contains("100 vertices"), "{err}");
}

#[test]
fn draw_bound_negative_count_errors() {
  let err = validate_draw_bound(-1, 20, 2000).expect_err("negative count must error");
  assert!(err.contains(">= 0"), "{err}");
  let err = validate_draw_bound(-1, 0, 0).expect_err("negative count must error without a bound too");
  assert!(err.contains(">= 0"), "{err}");
}
