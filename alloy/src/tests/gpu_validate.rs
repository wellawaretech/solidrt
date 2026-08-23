use std::collections::HashMap;

use crate::gpu::{
  resolve_draw_range, validate_draw_range, validate_params, validate_texture_bindings, BufferIds, BufferUpdate,
  DrawBounds, DrawRange, DrawUpdate, GpuLimits, IndexFormat, ParamValue, TextureBinding, UniformKind, UniformSlot, UniformTable,
};

fn table(entries: &[(&str, UniformKind)]) -> UniformTable {
  entries.iter().map(|(name, kind)| (name.to_string(), UniformSlot { kind: *kind, count: 1 })).collect()
}

fn array_table(entries: &[(&str, UniformKind, usize)]) -> UniformTable {
  entries.iter().map(|(name, kind, count)| (name.to_string(), UniformSlot { kind: *kind, count: *count })).collect()
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
fn params_array_uniforms_take_flat_arrays() {
  // vec3 uLight[4] expects 12 components flat; mat4 uBones[2] expects 32.
  let t = array_table(&[("uLight", UniformKind::Vec3, 4), ("uBones", UniformKind::Mat4, 2)]);
  assert_eq!(validate_params(&t, &[array("uLight", 12), array("uBones", 32)]), Ok(()));
  // A single element, or an off-by-one, errors with the array spelling.
  let err = validate_params(&t, &[array("uLight", 3)]).expect_err("one element for vec3[4] must error");
  assert!(err.contains("vec3[4]") && err.contains("expects 12"), "{err}");
  let err = validate_params(&t, &[array("uBones", 16)]).expect_err("one mat4 for mat4[2] must error");
  assert!(err.contains("mat4[2]") && err.contains("expects 32"), "{err}");
}

#[test]
fn params_sampler_and_unsupported_kinds_error() {
  let t = table(&[("uTex", UniformKind::Sampler2D), ("uIvec", UniformKind::Other(glow::INT_VEC2))]);
  let err = validate_params(&t, &[scalar("uTex", 1.0)]).expect_err("sampler via params must error");
  assert!(err.contains("bind it via textures"), "{err}");
  let err = validate_params(&t, &[scalar("uIvec", 1.0)]).expect_err("unsupported kind must error");
  assert!(err.contains("unsupported uniform type"), "{err}");
}

#[test]
fn texture_bindings_require_active_sampler2d() {
  let t = table(&[("uTex", UniformKind::Sampler2D), ("uColor", UniformKind::Vec4)]);
  assert_eq!(validate_texture_bindings(&t, &[TextureBinding::new("uTex", 7)]), Ok(()));
  let err = validate_texture_bindings(&t, &[TextureBinding::new("uColor", 7)]).expect_err("non-sampler must error");
  assert!(err.contains("uColor") && err.contains("not a sampler2D"), "{err}");
  let err = validate_texture_bindings(&t, &[TextureBinding::new("uTx", 7)]).expect_err("typo must error");
  assert!(err.contains("no active uniform named 'uTx'"), "{err}");
}

#[test]
fn texture_bindings_reject_sampler_arrays() {
  let t = array_table(&[("uTexes", UniformKind::Sampler2D, 4)]);
  let err = validate_texture_bindings(&t, &[TextureBinding::new("uTexes", 7)]).expect_err("sampler array must error");
  assert!(err.contains("sampler2D[4]") && err.contains("not a sampler2D"), "{err}");
}

fn range(first: i32, count: i32, instances: i32) -> DrawRange {
  DrawRange { first_vertex: first, vertex_count: count, instance_count: instances }
}

/// A plain vertex fetch bound: `size` bytes at `stride` bytes/vertex.
fn vbounds(stride: usize, size: usize) -> DrawBounds {
  DrawBounds { fetch: Some((stride, size)), indexed: false, instance: None }
}

/// An index fetch bound: `size` bytes at `elem` bytes/index.
fn ibounds(elem: usize, size: usize) -> DrawBounds {
  DrawBounds { fetch: Some((elem, size)), indexed: true, instance: None }
}

#[test]
fn draw_range_within_buffer_passes() {
  // 100 vertices at 20 bytes each in a 2000-byte buffer: exactly full.
  assert_eq!(validate_draw_range(range(0, 100, 1), vbounds(20, 2000)), Ok(()));
  assert_eq!(validate_draw_range(range(0, 0, 1), vbounds(20, 2000)), Ok(()));
  // A sub-range ending exactly at the buffer's end.
  assert_eq!(validate_draw_range(range(60, 40, 1), vbounds(20, 2000)), Ok(()));
  // Instances do not widen the vertex fetch (no instance buffer bound);
  // 0 instances (draw nothing) is legal.
  assert_eq!(validate_draw_range(range(0, 100, 1_000_000), vbounds(20, 2000)), Ok(()));
  assert_eq!(validate_draw_range(range(0, 100, 0), vbounds(20, 2000)), Ok(()));
  // Attributeless callers have no fetch bound: any non-negative range.
  assert_eq!(validate_draw_range(range(500, 1_000_000, 3), DrawBounds::default()), Ok(()));
}

#[test]
fn draw_range_past_buffer_end_errors() {
  let err = validate_draw_range(range(0, 101, 1), vbounds(20, 2000)).expect_err("one vertex past the end must error");
  assert!(err.contains("0..101") && err.contains("2020 bytes") && err.contains("100 vertices"), "{err}");
  // first shifts the fetch window even when the count alone would fit.
  let err = validate_draw_range(range(60, 41, 1), vbounds(20, 2000)).expect_err("first + count past the end must error");
  assert!(err.contains("60..101"), "{err}");
}

#[test]
fn draw_range_negative_fields_error() {
  let err = validate_draw_range(range(0, -1, 1), vbounds(20, 2000)).expect_err("negative count must error");
  assert!(err.contains("vertex count") && err.contains(">= 0"), "{err}");
  let err = validate_draw_range(range(-1, 3, 1), DrawBounds::default()).expect_err("negative first must error");
  assert!(err.contains("first vertex") && err.contains(">= 0"), "{err}");
  let err = validate_draw_range(range(0, 3, -1), DrawBounds::default()).expect_err("negative instances must error");
  assert!(err.contains("instance count") && err.contains(">= 0"), "{err}");
}

#[test]
fn resolve_derives_whole_buffer_and_tail() {
  // The create default: whole buffer, one instance.
  assert_eq!(resolve_draw_range(DrawRange::default(), vbounds(20, 2000)), Ok(range(0, 100, 1)));
  // With a first vertex, "the rest of the buffer" is the tail.
  assert_eq!(resolve_draw_range(range(60, -1, -1), vbounds(20, 2000)), Ok(range(60, 40, 1)));
  // Attributeless: nothing to derive from, so the default resolves to 0.
  assert_eq!(resolve_draw_range(DrawRange::default(), DrawBounds::default()), Ok(range(0, 0, 1)));
  // An explicit range passes through unchanged (validated, not derived).
  assert_eq!(resolve_draw_range(range(3, 5, 7), vbounds(20, 2000)), Ok(range(3, 5, 7)));
}

#[test]
fn resolve_rejects_bad_ranges() {
  let err = resolve_draw_range(range(101, -1, 1), vbounds(20, 2000)).expect_err("first past the end must error");
  assert!(err.contains("past the end") && err.contains("100 vertices"), "{err}");
  let err = resolve_draw_range(range(0, 101, 1), vbounds(20, 2000)).expect_err("explicit count past the end must error");
  assert!(err.contains("2020 bytes"), "{err}");
}

#[test]
fn instance_ranges_bound_and_derive() {
  // 8 instance records at 12 bytes each in a 96-byte instance buffer.
  let b = DrawBounds { instance: Some((12, 96)), ..vbounds(20, 2000) };
  assert_eq!(validate_draw_range(range(0, 100, 8), b), Ok(()));
  assert_eq!(validate_draw_range(range(0, 100, 0), b), Ok(()));
  let err = validate_draw_range(range(0, 100, 9), b).expect_err("one instance past the end must error");
  assert!(err.contains("9 instances") && err.contains("108 bytes") && err.contains("8 instances"), "{err}");
  // The default derives one instance per record of the instance buffer -
  // and stays 1 without one (the plain draw, covered above).
  assert_eq!(resolve_draw_range(DrawRange::default(), b), Ok(range(0, 100, 8)));
  // The instance bound also holds on an attributeless entry.
  let b = DrawBounds { instance: Some((12, 96)), ..DrawBounds::default() };
  assert_eq!(resolve_draw_range(DrawRange::default(), b), Ok(range(0, 0, 8)));
  let err = validate_draw_range(range(0, 0, 100), b).expect_err("instance bound must hold without vertices");
  assert!(err.contains("instance buffer holds 96 bytes"), "{err}");
}

#[test]
fn draw_update_merges_present_fields() {
  let current = range(10, 20, 30);
  let update = DrawUpdate { vertex_count: Some(25), ..DrawUpdate::default() };
  assert_eq!(current.merged(update, false), Ok(range(10, 25, 30)));
  assert_eq!(current.merged(DrawUpdate::default(), false), Ok(current));
  let all = DrawUpdate { first_vertex: Some(1), vertex_count: Some(2), instance_count: Some(3), ..DrawUpdate::default() };
  assert_eq!(current.merged(all, false), Ok(range(1, 2, 3)));
}

#[test]
fn draw_update_speaks_the_entry_vocabulary() {
  let current = range(10, 20, 30);
  // The index-named pair merges on an indexed entry (same fields underneath).
  let update = DrawUpdate { first_index: Some(3), index_count: Some(6), ..DrawUpdate::default() };
  assert_eq!(current.merged(update, true), Ok(range(3, 6, 30)));
  // The wrong pair errors instead of silently counting the other unit.
  let err = current
    .merged(DrawUpdate { vertex_count: Some(3), ..DrawUpdate::default() }, true)
    .expect_err("vertex keys on an indexed entry must error");
  assert!(err.contains("indexed") && err.contains("firstIndex/indexCount"), "{err}");
  let err = current
    .merged(DrawUpdate { first_index: Some(0), ..DrawUpdate::default() }, false)
    .expect_err("index keys on a plain entry must error");
  assert!(err.contains("no index buffer") && err.contains("firstVertex/vertexCount"), "{err}");
  // instanceCount is mode-free.
  let update = DrawUpdate { instance_count: Some(2), ..DrawUpdate::default() };
  assert_eq!(current.merged(update, true), Ok(range(10, 20, 2)));
}

#[test]
fn indexed_ranges_speak_indices() {
  // The same bound math at the index element size, with index nouns: 6
  // uint16 indices in a 12-byte buffer.
  assert_eq!(validate_draw_range(range(0, 6, 1), ibounds(2, 12)), Ok(()));
  let err = validate_draw_range(range(0, 7, 1), ibounds(2, 12)).expect_err("one index past the end must error");
  assert!(err.contains("index range") && err.contains("6 indices"), "{err}");
  let err = validate_draw_range(range(0, -1, 1), ibounds(2, 12)).expect_err("negative count must error");
  assert!(err.contains("index count"), "{err}");
  // Whole-buffer derivation from the index buffer's element count.
  assert_eq!(resolve_draw_range(DrawRange::default(), ibounds(2, 12)), Ok(range(0, 6, 1)));
  assert_eq!(resolve_draw_range(range(3, -1, 1), ibounds(2, 12)), Ok(range(3, 3, 1)));
}

#[test]
fn limits_texture_size_names_the_limit() {
  let l = GpuLimits { max_texture_size: 8192, ..GpuLimits::FLOOR };
  assert_eq!(l.check_texture_size(8192, 8192), Ok(()));
  assert_eq!(l.check_texture_size(1, 1), Ok(()));
  let err = l.check_texture_size(8193, 16).expect_err("oversize width must error");
  assert!(err.contains("8193x16") && err.contains("max texture size (8192)"), "{err}");
  let err = l.check_texture_size(16, 9000).expect_err("oversize height must error");
  assert!(err.contains("16x9000") && err.contains("8192"), "{err}");
}

#[test]
fn limits_texture_units_names_the_limit() {
  let l = GpuLimits::FLOOR;
  assert_eq!(l.check_texture_units(0), Ok(()));
  assert_eq!(l.check_texture_units(16), Ok(()));
  let err = l.check_texture_units(17).expect_err("over the unit cap must error");
  assert!(err.contains("17 sampler inputs") && err.contains("(16 per pass)"), "{err}");
}

#[test]
fn limits_vertex_attribs_names_the_limit() {
  let l = GpuLimits::FLOOR;
  assert_eq!(l.check_vertex_attribs(16), Ok(()));
  let err = l.check_vertex_attribs(17).expect_err("over the attribute cap must error");
  assert!(err.contains("17 vertex attributes") && err.contains("(16)"), "{err}");
}

#[test]
fn params_inactive_name_passes_and_is_not_listed_as_active() {
  let t = array_table(&[("uTime", UniformKind::Float, 1), ("uAlpha", UniformKind::Inactive, 1)]);
  assert_eq!(validate_params(&t, &[scalar("uAlpha", 0.5)]), Ok(()));
  let err = validate_params(&t, &[scalar("uTypo", 1.0)]).expect_err("undeclared must error");
  assert!(err.contains("(active: uTime)"), "{err}");
}

#[test]
fn texture_bindings_inactive_name_passes() {
  let t = table(&[("uMap", UniformKind::Inactive)]);
  assert_eq!(validate_texture_bindings(&t, &[TextureBinding::new("uMap", 7)]), Ok(()));
}

#[test]
fn declared_uniform_names_scans_source() {
  let src = r"#version 300 es
precision highp float;
// uniform float uCommented;
/* uniform vec2 uBlock;
   uniform vec2 uBlock2; */
uniform vec2 iResolution;
layout(std140) uniform vec4 uColor, uTint[4];
uniform sampler2D
  uMap;
uniform Lights { vec3 dir; } uLights;
void main() {}
";
  assert_eq!(
    crate::gpu::program::declared_uniform_names(src),
    vec!["iResolution", "uColor", "uTint", "uMap"].into_iter().map(String::from).collect::<Vec<_>>()
  );
}

#[test]
fn buffer_swap_replaces_filled_roles() {
  let ids = BufferIds { buffer: 1, index: Some((2, IndexFormat::U16)), instance_buffer: 3 };
  let next = ids
    .merged(BufferUpdate { buffer: None, index: Some((7, IndexFormat::U32)), instance_buffer: Some(9) })
    .expect("swap filled roles");
  assert_eq!(next, BufferIds { buffer: 1, index: Some((7, IndexFormat::U32)), instance_buffer: 9 });
  assert!(next.reads(9) && next.reads(7) && !next.reads(3) && !next.reads(0));
}

#[test]
fn buffer_swap_rejects_new_roles_and_zero_ids() {
  let plain = BufferIds { buffer: 1, index: None, instance_buffer: 0 };
  let err = plain.merged(BufferUpdate { instance_buffer: Some(5), ..Default::default() }).expect_err("no instance role");
  assert!(err.contains("instanceAttributes"), "{err}");
  let err = plain
    .merged(BufferUpdate { index: Some((5, IndexFormat::U16)), ..Default::default() })
    .expect_err("not indexed");
  assert!(err.contains("not indexed"), "{err}");
  let err = plain.merged(BufferUpdate { buffer: Some(0), ..Default::default() }).expect_err("zero id");
  assert!(err.contains("buffer id"), "{err}");
  let attributeless = BufferIds::default();
  let err = attributeless.merged(BufferUpdate { buffer: Some(4), ..Default::default() }).expect_err("no vertex role");
  assert!(err.contains("no attributes"), "{err}");
}
