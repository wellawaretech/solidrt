use std::collections::HashMap;

use crate::gpu::{
  check_cube_faces, mip_levels, mip_size, resolve_draw_range, validate_binding_shapes, validate_draw_range,
  validate_params, validate_texture_bindings, AttrFormat, BoundTexture, BufferBound, BufferIds, BufferUpdate,
  DrawBounds, DrawRange, DrawUpdate, GpuLimits, IndexFormat, ParamValue, StepMode, TextureBinding, TextureFormat,
  TextureShape, UniformKind, UniformSlot, UniformTable, CUBE_FACES,
};
use crate::gpu::vocab::AttrKind;

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
fn texture_bindings_require_active_sampler() {
  let t = table(&[
    ("uTex", UniformKind::Sampler2D),
    ("uShadow", UniformKind::Sampler2DShadow),
    ("uColor", UniformKind::Vec4),
  ]);
  assert_eq!(validate_texture_bindings(&t, &[TextureBinding::new("uTex", 7)]), Ok(()));
  // A comparison sampler binds the same way (the depth-format requirement
  // is the context's check, not this table-only one).
  assert_eq!(validate_texture_bindings(&t, &[TextureBinding::new("uShadow", 7)]), Ok(()));
  let err = validate_texture_bindings(&t, &[TextureBinding::new("uColor", 7)]).expect_err("non-sampler must error");
  assert!(err.contains("uColor") && err.contains("not a sampler"), "{err}");
  let err = validate_texture_bindings(&t, &[TextureBinding::new("uTx", 7)]).expect_err("typo must error");
  assert!(err.contains("no active uniform named 'uTx'"), "{err}");
}

#[test]
fn texture_bindings_reject_sampler_arrays() {
  let t = array_table(&[("uTexes", UniformKind::Sampler2D, 4)]);
  let err = validate_texture_bindings(&t, &[TextureBinding::new("uTexes", 7)]).expect_err("sampler array must error");
  assert!(err.contains("sampler2D[4]") && err.contains("not a sampler"), "{err}");
}

fn range(first: i32, count: i32, instances: i32) -> DrawRange {
  DrawRange { first_vertex: first, vertex_count: count, instance_count: instances }
}

/// One bound buffer: `size` bytes at `stride` bytes/record, at `step`.
fn bbound(step: StepMode, stride: usize, size: usize) -> BufferBound {
  BufferBound { step, stride, size }
}

/// A plain vertex fetch bound: `size` bytes at `stride` bytes/vertex.
fn vbounds(stride: usize, size: usize) -> DrawBounds {
  let mut b = DrawBounds::default();
  b.buffers[0] = bbound(StepMode::Vertex, stride, size);
  b
}

/// An index fetch bound: `size` bytes at `elem` bytes/index.
fn ibounds(elem: usize, size: usize) -> DrawBounds {
  DrawBounds { index: Some((elem, size)), ..DrawBounds::default() }
}

/// `bounds` with an instance-step buffer of `size` bytes at `stride`
/// bytes/record bound at index `at`.
fn with_instances(mut bounds: DrawBounds, at: usize, stride: usize, size: usize) -> DrawBounds {
  bounds.buffers[at] = bbound(StepMode::Instance, stride, size);
  bounds
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
  let err =
    validate_draw_range(range(60, 41, 1), vbounds(20, 2000)).expect_err("first + count past the end must error");
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
  let err =
    resolve_draw_range(range(0, 101, 1), vbounds(20, 2000)).expect_err("explicit count past the end must error");
  assert!(err.contains("2020 bytes"), "{err}");
}

#[test]
fn instance_ranges_bound_and_derive() {
  // 8 instance records at 12 bytes each in a 96-byte instance buffer.
  let b = with_instances(vbounds(20, 2000), 1, 12, 96);
  assert_eq!(validate_draw_range(range(0, 100, 8), b), Ok(()));
  assert_eq!(validate_draw_range(range(0, 100, 0), b), Ok(()));
  let err = validate_draw_range(range(0, 100, 9), b).expect_err("one instance past the end must error");
  assert!(err.contains("9 instances") && err.contains("108 bytes") && err.contains("8 instances"), "{err}");
  // The default derives one instance per record of the instance buffer -
  // and stays 1 without one (the plain draw, covered above).
  assert_eq!(resolve_draw_range(DrawRange::default(), b), Ok(range(0, 100, 8)));
  // The instance bound also holds on an attributeless entry.
  let b = with_instances(DrawBounds::default(), 0, 12, 96);
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
  let all =
    DrawUpdate { first_vertex: Some(1), vertex_count: Some(2), instance_count: Some(3), ..DrawUpdate::default() };
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
  let err = l.check_texture_size(0, 720).expect_err("zero width must error");
  assert!(err.contains("0x720") && err.contains("at least 1"), "{err}");
}

#[test]
fn limits_texture_units_names_the_limit() {
  let l = GpuLimits::FLOOR;
  assert_eq!(l.check_texture_units(0), Ok(()));
  assert_eq!(l.check_texture_units(16), Ok(()));
  let err = l.check_texture_units(17).expect_err("over the unit cap must error");
  assert!(err.contains("17 sampler inputs") && err.contains("(16 per pass)"), "{err}");
}

// A generated mip chain needs a color-renderable format: rgba16f's gate is
// the device's half-float renderability, every other format passes.
#[test]
fn limits_mipmap_gates_half_float_on_renderability() {
  use crate::gpu::texture::TextureFormat;

  let l = GpuLimits::FLOOR;
  assert_eq!(l.check_mipmap(TextureFormat::Rgba16f, false), Ok(()));
  assert_eq!(l.check_mipmap(TextureFormat::Rgba8Srgb, true), Ok(()));
  let err = l.check_mipmap(TextureFormat::Rgba16f, true).expect_err("half float mipmaps need the extension");
  assert!(err.contains("rgba16f") && err.contains("mipmap: true"), "{err}");
  let l = GpuLimits { half_float_renderable: true, ..GpuLimits::FLOOR };
  assert_eq!(l.check_mipmap(TextureFormat::Rgba16f, true), Ok(()));
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
    crate::gl::declared_uniform_names(src),
    vec!["iResolution", "uColor", "uTint", "uMap"].into_iter().map(String::from).collect::<Vec<_>>()
  );
}

#[test]
fn buffer_swap_replaces_filled_roles() {
  let ids = BufferIds { buffers: [1, 3, 0, 0, 0, 0, 0, 0], index: Some((2, IndexFormat::U16)) };
  let next = ids
    .merged(BufferUpdate { buffers: Some([1, 9, 0, 0, 0, 0, 0, 0]), index: Some((7, IndexFormat::U32)) })
    .expect("swap filled roles");
  assert_eq!(next, BufferIds { buffers: [1, 9, 0, 0, 0, 0, 0, 0], index: Some((7, IndexFormat::U32)) });
  assert!(next.reads(9) && next.reads(7) && !next.reads(3) && !next.reads(0));
  assert_eq!(next.bound().collect::<Vec<_>>(), vec![1, 9]);
}

#[test]
fn buffer_swap_rejects_new_roles_and_zero_ids() {
  let plain = BufferIds { buffers: [1, 0, 0, 0, 0, 0, 0, 0], index: None };
  let err = plain
    .merged(BufferUpdate { buffers: Some([1, 5, 0, 0, 0, 0, 0, 0]), index: None })
    .expect_err("an undeclared index must error");
  assert!(err.contains("buffer 1") && err.contains("not declared"), "{err}");
  let err = plain
    .merged(BufferUpdate { buffers: Some([0; 8]), index: None })
    .expect_err("dropping a declared buffer must error");
  assert!(err.contains("buffer 0") && err.contains("dropped"), "{err}");
  let err = plain.merged(BufferUpdate { index: Some((5, IndexFormat::U16)), buffers: None }).expect_err("not indexed");
  assert!(err.contains("not indexed"), "{err}");
}

#[test]
fn buffer_layouts_validate_and_bound() {
  use crate::gpu::{buffer_strides, validate_buffers, AttrFormat, BufferLayout, VertexAttr};
  let layouts = vec![
    BufferLayout::vertex(vec![("aPos".to_string(), AttrFormat::Float32x3), ("aUV".to_string(), AttrFormat::Unorm16x2)]),
    BufferLayout::instance(vec![
      ("iOffset".to_string(), AttrFormat::Float32x2),
      ("iColor".to_string(), AttrFormat::Unorm8x4),
    ]),
  ];
  assert_eq!(validate_buffers(&layouts), Ok(()));
  // Packed offsets run in list order; strides are the byte sums.
  assert_eq!(layouts[0].attributes[1].offset, 12);
  assert_eq!(layouts[0].stride, 16);
  let strides = buffer_strides(&layouts);
  assert_eq!((strides[0].step, strides[0].stride), (StepMode::Vertex, 16));
  assert_eq!((strides[1].step, strides[1].stride), (StepMode::Instance, 12));
  assert_eq!(strides[2].stride, 0);
  // A subset layout: positions alone out of a 36-byte record.
  let subset = BufferLayout {
    step: StepMode::Vertex,
    stride: 36,
    attributes: vec![VertexAttr { name: "aPos".to_string(), format: AttrFormat::Float32x3, offset: 0 }],
  };
  assert_eq!(validate_buffers(&[subset.clone()]), Ok(()));
  let mut past = subset.clone();
  past.attributes[0].offset = 28;
  let err = validate_buffers(&[past]).expect_err("an attribute past the stride must error");
  assert!(err.contains("does not fit"), "{err}");
  let mut odd = subset.clone();
  odd.stride = 30;
  let err = validate_buffers(&[odd]).expect_err("a stride off 4 must error");
  assert!(err.contains("multiple of 4"), "{err}");
  let twice = vec![layouts[0].clone(), BufferLayout::vertex(vec![("aPos".to_string(), AttrFormat::Float32x3)])];
  let err = validate_buffers(&twice).expect_err("a name twice must error");
  assert!(err.contains("aPos") && err.contains("twice"), "{err}");
  let many: Vec<BufferLayout> =
    (0..9).map(|i| BufferLayout::vertex(vec![(format!("a{i}"), AttrFormat::Float32)])).collect();
  let err = validate_buffers(&many).expect_err("past the buffer cap must error");
  assert!(err.contains("at most 8"), "{err}");
  // The draw bound derives from the tightest buffer of each step: two
  // vertex streams of 20 and 25 vertices, two instance buffers of 8 and 5
  // records.
  let mut b = vbounds(20, 400);
  b.buffers[1] = bbound(StepMode::Vertex, 8, 200);
  b.buffers[2] = bbound(StepMode::Instance, 12, 96);
  b.buffers[3] = bbound(StepMode::Instance, 12, 60);
  assert_eq!(b.fetch(), Some((20, 400)));
  assert_eq!(b.instance_limit(), Some((12, 60)));
  assert_eq!(resolve_draw_range(DrawRange::default(), b), Ok(range(0, 20, 5)));
  let err = validate_draw_range(range(0, 0, 6), b).expect_err("past the tightest instance buffer must error");
  assert!(err.contains("60 bytes") && err.contains("5 instances"), "{err}");
  let err = validate_draw_range(range(0, 21, 1), b).expect_err("past the tightest vertex stream must error");
  assert!(err.contains("20 vertices"), "{err}");
}

fn bound(shape: TextureShape, format: TextureFormat) -> BoundTexture {
  BoundTexture { shape, format }
}

/// A registry of three ids: 1 a 2D rgba8, 2 a cube map, 3 a depth texture.
fn lookup(id: u64) -> Option<BoundTexture> {
  match id {
    1 => Some(bound(TextureShape::D2, TextureFormat::Rgba8)),
    2 => Some(bound(TextureShape::Cube, TextureFormat::Rgba8)),
    3 => Some(bound(TextureShape::D2, TextureFormat::Depth24)),
    _ => None,
  }
}

#[test]
fn binding_shapes_match_sampler_kinds() {
  let t = table(&[
    ("uTex", UniformKind::Sampler2D),
    ("uEnv", UniformKind::SamplerCube),
    ("uShadow", UniformKind::Sampler2DShadow),
  ]);
  let ok = [TextureBinding::new("uTex", 1), TextureBinding::new("uEnv", 2), TextureBinding::new("uShadow", 3)];
  assert_eq!(validate_binding_shapes(&t, &ok, lookup), Ok(()));
  // A depth id on a plain sampler2D is the raw depth read: legal.
  assert_eq!(validate_binding_shapes(&t, &[TextureBinding::new("uTex", 3)], lookup), Ok(()));
  // An unregistered id is not this rule's concern.
  assert_eq!(validate_binding_shapes(&t, &[TextureBinding::new("uEnv", 9)], lookup), Ok(()));
}

#[test]
fn binding_shapes_reject_cross_shape_both_ways() {
  let t = table(&[("uTex", UniformKind::Sampler2D), ("uEnv", UniformKind::SamplerCube)]);
  let err = validate_binding_shapes(&t, &[TextureBinding::new("uTex", 2)], lookup).expect_err("cube on 2D must error");
  assert!(err.contains("texture 2 is a cube map") && err.contains("samplerCube"), "{err}");
  let err = validate_binding_shapes(&t, &[TextureBinding::new("uEnv", 1)], lookup).expect_err("2D on cube must error");
  assert!(err.contains("uEnv") && err.contains("createCubeTexture"), "{err}");
}

#[test]
fn binding_shapes_require_depth_behind_compare_sampler() {
  let t = table(&[("uShadow", UniformKind::Sampler2DShadow)]);
  let err =
    validate_binding_shapes(&t, &[TextureBinding::new("uShadow", 1)], lookup).expect_err("color on shadow must error");
  assert!(err.contains("sampler2DShadow") && err.contains("depthTexture"), "{err}");
  let err =
    validate_binding_shapes(&t, &[TextureBinding::new("uShadow", 2)], lookup).expect_err("cube on shadow must error");
  assert!(err.contains("cube map"), "{err}");
}

#[test]
fn sampler_cube_is_a_sampler_kind() {
  assert!(UniformKind::SamplerCube.is_sampler());
  assert_eq!(UniformKind::SamplerCube.sampler_shape(), Some(TextureShape::Cube));
  assert_eq!(UniformKind::Sampler2DShadow.sampler_shape(), Some(TextureShape::D2));
  assert_eq!(UniformKind::Vec4.sampler_shape(), None);
  let t = table(&[("uEnv", UniformKind::SamplerCube)]);
  let err = validate_params(&t, &[scalar("uEnv", 1.0)]).expect_err("sampler via params must error");
  assert!(err.contains("samplerCube") && err.contains("bind it via textures"), "{err}");
}

// The cube face list: six base faces, or the full mip chain level-major.
fn cube_faces(size: u32, levels: u32) -> Vec<Vec<u8>> {
  let mut faces = Vec::new();
  for level in 0..levels {
    let edge = mip_size(size, level) as usize;
    for _ in 0..CUBE_FACES {
      faces.push(vec![0u8; edge * edge * 4]);
    }
  }
  faces
}

#[test]
fn mip_chain_counts_and_edges() {
  assert_eq!(mip_levels(1), 1);
  assert_eq!(mip_levels(4), 3);
  assert_eq!(mip_levels(5), 3);
  assert_eq!(mip_levels(128), 8);
  assert_eq!(mip_size(5, 1), 2);
  assert_eq!(mip_size(5, 2), 1);
  assert_eq!(mip_size(4, 7), 1);
}

#[test]
fn cube_faces_base_or_full_chain() {
  let rgba = TextureFormat::Rgba8;
  assert_eq!(check_cube_faces(4, &cube_faces(4, 1), rgba).expect("six faces"), 1);
  assert_eq!(check_cube_faces(4, &cube_faces(4, 3), rgba).expect("full chain"), 3);
  let err = check_cube_faces(4, &cube_faces(4, 2), rgba).expect_err("a partial chain must error");
  assert!(err.contains("full 3-level mip chain of 18 faces") && err.contains("got 12"), "{err}");
  let mut short = cube_faces(4, 3);
  short[7] = vec![0u8; 4];
  let err = check_cube_faces(4, &short, rgba).expect_err("a wrong level edge must error");
  assert!(err.contains("face 1 of level 1") && err.contains("expected 16 (2x2 rgba8)"), "{err}");
}

// The vertex format table: WebGPU's rule that the format decides which
// shader input family it feeds, checked without a GL context.

fn attr(name: &str) -> AttrFormat {
  AttrFormat::parse(name).expect(name)
}

#[test]
fn attr_formats_feed_their_own_kind() {
  // A float `in vec4` takes any float or normalized 4-component row.
  for name in ["float32x4", "float16x4", "unorm8x4", "snorm16x4"] {
    assert_eq!(attr(name).feeds("aColor", AttrFormat::Float32x4), Ok(()), "{name}");
  }
  // An integer `in` takes unnormalized rows of its signedness at any width.
  for name in ["uint8x4", "uint16x4", "uint32x4"] {
    assert_eq!(attr(name).feeds("aJoints", AttrFormat::Uint32x4), Ok(()), "{name}");
  }
  for name in ["sint8x4", "sint16x4", "sint32x4"] {
    assert_eq!(attr(name).feeds("aCell", AttrFormat::Sint32x4), Ok(()), "{name}");
  }
  assert_eq!(attr("uint32").feeds("aId", AttrFormat::Uint32), Ok(()));
}

#[test]
fn attr_formats_never_cross_kinds() {
  // Unnormalized integers no longer feed a float input.
  let err = attr("uint8x4").feeds("aJoints", AttrFormat::Float32x4).expect_err("uint into vec4");
  assert!(err.contains("uint/uvec*"), "{err}");
  // A float or normalized row cannot feed an integer input.
  assert!(attr("float32x4").feeds("aJoints", AttrFormat::Uint32x4).is_err());
  assert!(attr("unorm8x4").feeds("aJoints", AttrFormat::Uint32x4).is_err());
  // Signedness is part of the kind.
  assert!(attr("sint32x4").feeds("aJoints", AttrFormat::Uint32x4).is_err());
  // Component count is checked first.
  let err = attr("uint32x2").feeds("aId", AttrFormat::Uint32x4).expect_err("count");
  assert!(err.contains("2 components"), "{err}");
}

#[test]
fn attr_format_sizes_follow_component_width() {
  assert_eq!(attr("uint32x3").bytes(), 12);
  assert_eq!(attr("sint32").bytes(), 4);
  assert_eq!(attr("sint16x2").bytes(), 4);
  assert_eq!(attr("sint8x4").bytes(), 4);
  assert_eq!(attr("uint32x4").kind(), AttrKind::Uint);
  assert_eq!(attr("snorm8x4").kind(), AttrKind::Float);
}
