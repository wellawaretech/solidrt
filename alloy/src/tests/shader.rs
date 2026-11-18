//! The `<shader>` element's paint walk (kinds/shader.rs), read back from the
//! frame's display list (`DisplayList::ops` and `renderer::oplog::placed`):
//! one paint shader draw at its box, the rounded clip around it, nothing
//! for an id the registry does not hold; its damage and the texture ids it
//! references. No pixels: those are alloy/examples/paint_shader.rs.
use crate::context::PaintShaderEntry;
use crate::gpu::{DirtySet, ParamValue, SamplerState, TextureBinding, TextureEntry, TextureFormat, UniformTable};
use crate::renderer::oplog::{placed, Op};
use crate::renderer::prelude::*;
use crate::renderer::{ClipOperation, DisplayList, DisplayListBuilder, ISize, Point, Rect, Size};
use crate::rendertree::composite::paint_phase;
use crate::rendertree::*;
use std::rc::Rc;
use std::sync::Arc;
use taffy::prelude::*;

const ROOT: u64 = 1;
const SHADER: u64 = 2;
// The paint shader id the registry holds, and one it does not.
const PAINT_ID: u64 = 7;
const MISSING_ID: u64 = 8;
// A texture id the shader samples as an input.
const INPUT_ID: u64 = 11;
// The input texture's dummy GL name and size: no GL ever sees either.
const INPUT_NAME: u32 = 5;
const INPUT_SIZE: i64 = 16;
// The shader's box, offset inside the root by its padding.
const PAD: f32 = 10.0;
const BOX_W: f32 = 120.0;
const BOX_H: f32 = 80.0;
const RADIUS: f32 = 12.0;

// A Context with no raster thread behind it, holding a paint shader handle
// with no program behind it and one input texture.
fn headless() -> crate::Context {
  let stats = Arc::new(crate::raster::RasterStats::new());
  let (tx, _rx) = std::sync::mpsc::channel();
  let alloy = crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None);
  alloy.paint_shaders.insert(
    PAINT_ID,
    PaintShaderEntry { handle: crate::renderer::test_paint_shader(), uniforms: Rc::new(UniformTable::new()) },
  );
  let texture = crate::renderer::test_texture(INPUT_NAME, ISize::new(INPUT_SIZE, INPUT_SIZE));
  alloy.textures.insert(
    INPUT_ID,
    TextureEntry::d2(texture, INPUT_SIZE as u32, INPUT_SIZE as u32, SamplerState::default(), TextureFormat::Rgba8),
  );
  alloy
}

// root(1, 400x300, padded) > shader(2, BOX_W x BOX_H) drawing `src`.
fn tree_with(src: u64, radius: Option<[f32; 4]>) -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, View::default().with_layout());
  tree.create_node(SHADER, Shader::default().with_layout());
  tree.insert_node(ROOT, SHADER, None).expect("insert");
  tree.root = Some(ROOT);
  let root = tree.node_mut(ROOT).style_mut().expect("root");
  root.size = taffy::Size { width: length(400.0), height: length(300.0) };
  root.padding = taffy::Rect { left: length(PAD), right: length(PAD), top: length(PAD), bottom: length(PAD) };
  tree.node_mut(SHADER).style_mut().expect("shader").size = taffy::Size { width: length(BOX_W), height: length(BOX_H) };
  tree.edit(SHADER, |el| {
    let ElementKind::Shader(s) = &mut el.kind else { panic!("a shader node") };
    s.set_src(Some(src));
    s.set_textures(vec![TextureBinding::new("uTex", INPUT_ID)]);
    s.set_radius(radius)
  });
  tree
}

fn drawn(tree: &mut RenderTree) -> DisplayList {
  let platform = PlatformContext::new(Vec::new());
  platform.set_window_size(400.0, 300.0);
  let alloy = headless();
  let mut builder = DisplayListBuilder::new(None);
  paint_phase(&mut builder, tree, &platform, &alloy);
  builder.build().expect("the frame builds")
}

fn the_box() -> Rect {
  Rect::new(Point::new(PAD, PAD), Size::new(BOX_W, BOX_H))
}

#[test]
fn a_shader_records_one_draw_at_its_box() {
  let mut tree = tree_with(PAINT_ID, None);
  let list = drawn(&mut tree);
  let ops = list.ops();
  let draws: Vec<_> = placed(&ops).into_iter().filter(|p| matches!(p.op, Op::DrawShader { .. })).collect();
  assert_eq!(draws.len(), 1, "one paint shader draw");
  assert_eq!(draws[0].device_rect(), Some(the_box()));
  assert!(!placed(&ops).iter().any(|p| matches!(p.op, Op::ClipRoundedRect { .. })), "no radius, no rounded clip");
}

#[test]
fn a_radius_records_the_rounded_clip_around_the_draw() {
  let mut tree = tree_with(PAINT_ID, Some([RADIUS; 4]));
  let list = drawn(&mut tree);
  let ops = list.ops();
  let at = |want: fn(&Op) -> bool| ops.iter().position(want).expect("the op is recorded");
  let clip = at(|op| matches!(op, Op::ClipRoundedRect { .. }));
  let draw = at(|op| matches!(op, Op::DrawShader { .. }));
  assert!(clip < draw, "the clip comes first");
  assert!(matches!(ops[..clip].last(), Some(Op::Save)), "a save opens the clip");
  assert!(matches!(ops.get(draw + 1), Some(Op::Restore)), "a restore closes it after the draw");
  let rounded = placed(&ops).into_iter().find(|p| matches!(p.op, Op::ClipRoundedRect { .. })).expect("placed clip");
  assert_eq!(rounded.device_rect(), Some(the_box()));
  let Op::ClipRoundedRect { radii, op, .. } = rounded.op else { unreachable!() };
  assert_eq!(*op, ClipOperation::Intersect);
  assert_eq!(radii.top_left, Point::new(RADIUS, RADIUS));
}

#[test]
fn a_missing_paint_shader_records_nothing() {
  let mut tree = tree_with(MISSING_ID, Some([RADIUS; 4]));
  let list = drawn(&mut tree);
  let ops = list.ops();
  assert!(
    !ops.iter().any(|op| matches!(op, Op::DrawShader { .. } | Op::ClipRoundedRect { .. })),
    "no draw and no clip for an id the registry does not hold"
  );
}

#[test]
fn params_and_inputs_are_paint_damage() {
  let mut shader = Shader::default();
  assert_eq!(shader.set_params(vec![("uMix".to_string(), ParamValue::Scalar(0.5))]), Damage::Paint);
  assert_eq!(shader.params.len(), 1);
  assert_eq!(shader.params[0].value, vec![0.5]);
  assert_eq!(shader.set_textures(vec![TextureBinding::new("uTex", INPUT_ID)]), Damage::Paint);
  assert_eq!(shader.set_src(Some(PAINT_ID)), Damage::Paint);
}

#[test]
fn a_shader_references_its_inputs() {
  let mut tree = tree_with(PAINT_ID, None);
  assert!(tree.referenced_texture_ids().contains(&INPUT_ID));
  // A change of the input is paint damage on the node; an unrelated id
  // changes nothing.
  let mut other = DirtySet::new();
  other.insert(INPUT_ID + 1);
  assert!(!tree.texture_content_changed(&other));
  let mut changed = DirtySet::new();
  changed.insert(INPUT_ID);
  assert!(tree.texture_content_changed(&changed));
}
