// Paths and their meshes (renderer/path.rs, renderer/mesh.rs,
// renderer/walk.rs): the builder's tolerance of the contract's walks, the
// tessellation under the fill rules and the stroke metrics, the scale
// bucket, the mesh cache on the path, the walk's mesh draws (a retained
// mesh by handle with its map, one batch each), the GPU slot rule and the
// draw report's mesh counts, with no GL context.

use crate::prelude::*;
use crate::renderer::mesh::MESH_CACHE_ENTRIES;
use crate::renderer::mesh::{bucket_tolerance, gpu_slot, scale_bucket, tessellate, GpuMesh, GpuSlot, Mesh, MeshStyle};
use crate::renderer::ramps::RampTable;
use crate::renderer::walk::{walk, BatchProgram, GradientClass, Projective, Walked, TILE_CLAMP};
use crate::renderer::{report, test_texture, DeletionQueue, Freed};
use crate::{Affine, ClipOperation, Color, ColorSource, DisplayListBuilder, DrawStyle, FillType, ISize};
use crate::{GradientKind, TextureSampling, TileMode};
use crate::{Matrix, Paint, Path, PathBuilder, Point, Rect, RoundingRadii, Size, StrokeCap, StrokeJoin};
use euclid::default::{Box2D, Point2D, Transform2D};
use lyon_path::Event;
use std::sync::Arc;

// How far a computed value may sit from the expected one: float noise.
const EPS: f32 = 1e-4;

// The target every walk draws into, in device pixels.
const TARGET: f32 = 100.0;

// A star's outer radius and its center, for the fill rule tests.
const STAR_RADIUS: f32 = 40.0;
// How far in from a star's tip the covered probe sits, as a fraction of the
// radius: inside the tip's triangle, away from its edges.
const TIP_INSET: f32 = 0.9;
const STAR_CENTER: f32 = 50.0;

// The stroke width of the line tests.
const LINE_WIDTH: f32 = 10.0;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn near(got: f32, want: f32) -> bool {
  (got - want).abs() < EPS
}

fn walked(b: DisplayListBuilder) -> Walked {
  walk(
    &b.build().expect("a builder always builds a list"),
    ISize::new(TARGET as i64, TARGET as i64),
    &mut RampTable::new(),
  )
}

// A solid opaque fill.
fn solid() -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
  paint
}

// The closed square at (x, y) with side `s`.
fn square(x: f32, y: f32, s: f32) -> Path {
  let mut b = PathBuilder::default();
  b.move_to(Point::new(x, y))
    .line_to(Point::new(x + s, y))
    .line_to(Point::new(x + s, y + s))
    .line_to(Point::new(x, y + s))
    .close();
  b.build(FillType::NonZero)
}

// The open segment from (x0, y) to (x1, y).
fn segment(x0: f32, x1: f32, y: f32) -> Path {
  let mut b = PathBuilder::default();
  b.move_to(Point::new(x0, y)).line_to(Point::new(x1, y));
  b.build(FillType::NonZero)
}

// A five-point star drawn in one stroke (each point joined to the one two
// along), so its inner pentagon winds twice: inside under NonZero, outside
// under EvenOdd.
fn star(fill: FillType) -> Path {
  let point = |k: usize| {
    let angle = (-90.0f32 + 144.0 * k as f32).to_radians();
    Point::new(STAR_CENTER + STAR_RADIUS * angle.cos(), STAR_CENTER + STAR_RADIUS * angle.sin())
  };
  let mut b = PathBuilder::default();
  b.move_to(point(0));
  for k in 1..5 {
    b.line_to(point(k));
  }
  b.close();
  b.build(fill)
}

fn stroke(width: f32, cap: StrokeCap) -> MeshStyle {
  MeshStyle::Stroke { width, cap, join: StrokeJoin::Miter, miter: crate::contract::DEFAULT_STROKE_MITER }
}

// Whether the triangle (a, b, c) contains p, edges included.
fn in_triangle(a: [f32; 2], b: [f32; 2], c: [f32; 2], p: [f32; 2]) -> bool {
  let side = |u: [f32; 2], v: [f32; 2]| (v[0] - u[0]) * (p[1] - u[1]) - (v[1] - u[1]) * (p[0] - u[0]);
  let (d0, d1, d2) = (side(a, b), side(b, c), side(c, a));
  let negative = d0 < 0.0 || d1 < 0.0 || d2 < 0.0;
  let positive = d0 > 0.0 || d1 > 0.0 || d2 > 0.0;
  !(negative && positive)
}

// Whether any triangle of `indices` over `vertices` contains p.
fn triangles_cover(vertices: &[[f32; 2]], indices: &[u32], p: [f32; 2]) -> bool {
  indices.chunks(3).any(|t| in_triangle(vertices[t[0] as usize], vertices[t[1] as usize], vertices[t[2] as usize], p))
}

// Whether the mesh, in the path's space, covers p.
fn mesh_covers(mesh: &Mesh, p: [f32; 2]) -> bool {
  triangles_cover(&mesh.vertices, &mesh.indices, p)
}

// Whether any mesh the walk draws, its vertices projected to device space
// under its draw's map, covers p.
fn covers(walked: &Walked, p: [f32; 2]) -> bool {
  walked.mesh_draws.iter().any(|draw| {
    let device: Vec<[f32; 2]> = draw
      .mesh
      .vertices
      .iter()
      .map(|v| draw.map.project(Point::new(v[0], v[1])).expect("the test meshes lie in front of the eye"))
      .map(|d| [d.x, d.y])
      .collect();
    triangles_cover(&device, &draw.mesh.indices, p)
  })
}

// `Path::mesh` for a path that has a mesh: the mesh and the miss flag.
fn mesh_hit(path: &Path, style: MeshStyle, bucket: i32) -> (Arc<Mesh>, bool) {
  let (mesh, missed) = path.mesh(style, bucket);
  (mesh.expect("the path has a mesh"), missed)
}

fn mesh_of(path: &Path, style: MeshStyle) -> Mesh {
  tessellate(path.geometry(), path.fill_type(), style, bucket_tolerance(0)).expect("lyon tessellates the test paths")
}

fn eval(rows: &Affine, x: f32, y: f32) -> (f32, f32) {
  let dot = |r: [f32; 3]| r[0] * x + r[1] * y + r[2];
  (dot(rows[0]), dot(rows[1]))
}

#[test]
fn a_segment_before_any_move_to_begins_at_the_origin() {
  let mut b = PathBuilder::default();
  b.line_to(Point::new(10.0, 5.0));
  let path = b.build(FillType::NonZero);
  let first = path.geometry().iter().next();
  assert_eq!(first, Some(Event::Begin { at: Point::new(0.0, 0.0) }));
}

#[test]
fn a_second_close_is_nothing() {
  let mut b = PathBuilder::default();
  b.move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).line_to(Point::new(10.0, 10.0));
  b.close().close();
  let path = b.build(FillType::NonZero);
  let ends: Vec<bool> = path
    .geometry()
    .iter()
    .filter_map(|e| match e {
      Event::End { close, .. } => Some(close),
      _ => None,
    })
    .collect();
  assert_eq!(ends, vec![true], "one closed subpath");
}

#[test]
fn build_ends_an_open_subpath() {
  let path = segment(0.0, 10.0, 0.0);
  let last = path.geometry().iter().last();
  assert!(matches!(last, Some(Event::End { close: false, .. })), "{last:?}");
}

#[test]
fn the_bounds_are_the_control_points_box() {
  let mut b = PathBuilder::default();
  b.move_to(Point::new(0.0, 0.0)).cubic_to(Point::new(0.0, 100.0), Point::new(100.0, 100.0), Point::new(100.0, 0.0));
  let path = b.build(FillType::NonZero);
  let bounds = path.bounds().expect("a path with a segment has bounds");
  assert_eq!(bounds, Box2D::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)));
  assert!(PathBuilder::default().build(FillType::NonZero).bounds().is_none(), "an empty path has none");
}

#[test]
fn a_square_fill_is_two_triangles() {
  let mesh = mesh_of(&square(0.0, 0.0, 10.0), MeshStyle::Fill);
  assert_eq!(mesh.vertices.len(), 4);
  assert_eq!(mesh.indices.len(), 6);
  assert_eq!(mesh.bounds, Box2D::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)));
}

#[test]
fn a_stroke_covers_its_width_and_its_caps() {
  let line = segment(0.0, 100.0, 50.0);
  let half = LINE_WIDTH * 0.5;
  let tolerance = bucket_tolerance(0);
  let butt = mesh_of(&line, stroke(LINE_WIDTH, StrokeCap::Butt)).bounds;
  let within = |got: f32, want: f32| (got - want).abs() <= tolerance;
  assert!(within(butt.min.x, 0.0) && within(butt.max.x, 100.0), "{butt:?}");
  assert!(within(butt.min.y, 50.0 - half) && within(butt.max.y, 50.0 + half), "{butt:?}");
  let square_cap = mesh_of(&line, stroke(LINE_WIDTH, StrokeCap::Square)).bounds;
  assert!(within(square_cap.min.x, -half) && within(square_cap.max.x, 100.0 + half), "{square_cap:?}");
}

#[test]
fn the_fill_rule_decides_the_star_center() {
  let center = [STAR_CENTER, STAR_CENTER];
  let non_zero = mesh_of(&star(FillType::NonZero), MeshStyle::Fill);
  assert!(mesh_covers(&non_zero, center), "the inner pentagon winds twice: inside under NonZero");
  let even_odd = mesh_of(&star(FillType::EvenOdd), MeshStyle::Fill);
  assert!(!mesh_covers(&even_odd, center), "and outside under EvenOdd");
  // A point of the star is covered under both.
  let tip = [STAR_CENTER, STAR_CENTER - STAR_RADIUS * TIP_INSET];
  assert!(mesh_covers(&non_zero, tip) && mesh_covers(&even_odd, tip));
}

#[test]
fn the_scale_bucket_rounds_the_stretch_up_to_a_power_of_two() {
  assert_eq!(scale_bucket(&Transform2D::identity()), Some(0));
  assert_eq!(scale_bucket(&Transform2D::scale(2.0, 2.0)), Some(1));
  assert_eq!(scale_bucket(&Transform2D::scale(1.5, 1.5)), Some(1));
  // The larger axis decides.
  assert_eq!(scale_bucket(&Transform2D::scale(1.0, 1.5)), Some(1));
  // The ceiling of log2 0.3, about -1.74.
  assert_eq!(scale_bucket(&Transform2D::scale(0.3, 0.3)), Some(-1));
  assert_eq!(scale_bucket(&Transform2D::scale(0.0, 0.0)), None);
  assert_eq!(scale_bucket(&Transform2D::scale(f32::INFINITY, 1.0)), None);
  assert!(near(bucket_tolerance(1), bucket_tolerance(0) * 0.5));
}

#[test]
fn the_cache_hits_on_its_key_and_drops_the_oldest() {
  let path = square(0.0, 0.0, 10.0);
  let (first, missed) = mesh_hit(&path, MeshStyle::Fill, 0);
  assert!(missed, "the first lookup tessellates");
  let (again, missed) = mesh_hit(&path, MeshStyle::Fill, 0);
  assert!(!missed && Arc::ptr_eq(&first, &again), "the second is the cached mesh");
  // A clone shares the cache.
  let (shared, missed) = mesh_hit(&path.clone(), MeshStyle::Fill, 0);
  assert!(!missed && Arc::ptr_eq(&first, &shared));
  // Another width and another bucket are other keys.
  assert!(mesh_hit(&path, stroke(2.0, StrokeCap::Butt), 0).1);
  assert!(!mesh_hit(&path, stroke(2.0, StrokeCap::Butt), 0).1);
  assert!(mesh_hit(&path, stroke(3.0, StrokeCap::Butt), 0).1);
  assert!(mesh_hit(&path, MeshStyle::Fill, 1).1);

  // MESH_CACHE_ENTRIES + 1 keys from a fresh path: the first is evicted.
  let path = square(0.0, 0.0, 10.0);
  for bucket in 0..=MESH_CACHE_ENTRIES as i32 {
    assert!(mesh_hit(&path, MeshStyle::Fill, bucket).1);
  }
  assert!(mesh_hit(&path, MeshStyle::Fill, 0).1, "the oldest key was dropped");
  assert!(!mesh_hit(&path, MeshStyle::Fill, MESH_CACHE_ENTRIES as i32).1);
}

#[test]
fn a_path_walks_into_one_mesh_draw_and_redraws_from_its_cache() {
  let path = square(0.0, 0.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 20.0).draw_path(&path, &solid());
  let list = b.build().expect("a builder always builds a list");
  let size = ISize::new(TARGET as i64, TARGET as i64);
  let out = walk(&list, size, &mut RampTable::new());
  let mesh = mesh_hit(&path, MeshStyle::Fill, 0).0;
  assert_eq!(out.mesh_draws.len(), 1);
  let draw = &out.mesh_draws[0];
  assert!(Arc::ptr_eq(&draw.mesh, &mesh), "the draw holds the path's cached mesh, in the path's space");
  assert_eq!(draw.map, Projective::from_matrix(&Matrix::translation(10.0, 20.0, 0.0)));
  assert_eq!(draw.color, [1.0, 0.0, 0.0, 1.0]);
  assert_eq!(draw.clip, [0.0, 0.0, TARGET, TARGET]);
  assert!(draw.rounded.is_none() && draw.gradient.is_none() && draw.texture.is_none() && draw.texel.is_none());
  assert_eq!(out.batches.len(), 1);
  let batch = &out.batches[0];
  assert_eq!((batch.program, batch.start, batch.count), (BatchProgram::Mesh, 0, 1));
  assert!(batch.texture.is_none() && batch.gradient.is_none());
  assert_eq!((out.meshes, out.tessellated), (1, 1));
  assert!(covers(&out, [15.0, 25.0]), "the square's center, offset");
  assert!(!covers(&out, [5.0, 5.0]), "the square's untranslated place");
  let r = report(&out);
  assert_eq!((r.meshes, r.tessellated, r.instances, r.draw_calls), (1, 1, 0, 1));
  assert_eq!(out.scissor, Some(Box2D::new(Point2D::new(0, 0), Point2D::new(TARGET as i32, TARGET as i32))));

  // The same list again: the path's cache answers with the same mesh.
  let again = walk(&list, size, &mut RampTable::new());
  assert_eq!((again.meshes, again.tessellated), (1, 0));
  assert!(Arc::ptr_eq(&again.mesh_draws[0].mesh, &out.mesh_draws[0].mesh), "the retained mesh is handed out again");
  assert_eq!((report(&again).meshes, report(&again).tessellated), (1, 0));
}

#[test]
fn a_path_outside_the_clip_tessellates_nothing() {
  let path = square(0.0, 0.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(50.0, 50.0, 10.0, 10.0), ClipOperation::Intersect).draw_path(&path, &solid());
  let out = walked(b);
  assert_eq!((out.meshes, out.tessellated), (0, 0));
  assert!(out.mesh_draws.is_empty() && out.batches.is_empty());
  assert_eq!(out.scissor, None);
  assert!(mesh_hit(&path, MeshStyle::Fill, 0).1, "the walk left the cache empty");
}

#[test]
fn a_stroke_outset_by_its_reach_is_kept() {
  // The segment's own box misses the clip below it; its stroke reaches in.
  let path = segment(0.0, 50.0, 10.0);
  let mut paint = solid();
  paint.set_draw_style(DrawStyle::Stroke).set_stroke_width(LINE_WIDTH);
  let mut b = DisplayListBuilder::new(None);
  b.clip_rect(&rect(0.0, 12.0, 50.0, 10.0), ClipOperation::Intersect).draw_path(&path, &paint);
  let out = walked(b);
  assert_eq!(out.meshes, 1);
}

#[test]
fn two_paths_never_share_a_batch() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_path(&square(0.0, 0.0, 10.0), &solid()).draw_path(&square(20.0, 0.0, 10.0), &solid());
  let out = walked(b);
  let ranges: Vec<(BatchProgram, usize, usize)> = out.batches.iter().map(|b| (b.program, b.start, b.count)).collect();
  assert_eq!(ranges, vec![(BatchProgram::Mesh, 0, 1), (BatchProgram::Mesh, 1, 1)], "one draw call per path");
  assert_eq!((out.meshes, out.mesh_draws.len()), (2, 2));
  assert!(covers(&out, [25.0, 5.0]));
}

#[test]
fn a_gradient_path_carries_the_gradients_own_rows() {
  let mut gradient = solid();
  let source = ColorSource::linear_gradient(
    Point::zero(),
    Point::new(TARGET, 0.0),
    &[Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  );
  gradient.set_color(Color::new_srgba(0.2, 0.4, 0.6, 0.5)).set_color_source(&source);
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 0.0).draw_path(&square(0.0, 0.0, 10.0), &gradient).draw_path(&square(20.0, 0.0, 10.0), &solid());
  let out = walked(b);
  assert_eq!(out.batches.len(), 2, "{:?}", out.batches);
  let class = GradientClass { kind: GradientKind::Linear as i32 as f32, tile: TILE_CLAMP };
  assert_eq!((out.batches[0].gradient, out.batches[1].gradient), (Some(class), None));
  let draw = out.mesh_draws[0].gradient.as_ref().expect("the first draw carries the gradient");
  assert_eq!(draw.rows, [draw.gradient.x_row, draw.gradient.y_row], "rows over the path's space");
  // The path's x 0 is the gradient's start, whatever the translation.
  assert!(near(eval(&draw.rows, 0.0, 0.0).0, 0.0), "{:?}", draw.rows);
  // The paint's alpha in all four channels.
  assert_eq!(out.mesh_draws[0].color, [0.5; 4]);
  assert!(out.mesh_draws[1].gradient.is_none());
}

#[test]
fn an_image_path_maps_its_path_corner_to_the_first_texel() {
  let texture = test_texture(1, ISize::new(20, 20));
  let mut paint = solid();
  let to_draw = Matrix::translation(5.0, 5.0, 0.0);
  let source = ColorSource::image(&texture, TileMode::Clamp, TileMode::Clamp, TextureSampling::Linear, Some(&to_draw));
  paint.set_color_source(&source);
  let mut b = DisplayListBuilder::new(None);
  b.translate(10.0, 20.0).draw_path(&square(5.0, 5.0, 20.0), &paint);
  let out = walked(b);
  assert_eq!(out.batches.len(), 1);
  let name = texture.gl_name().expect("an anvil texture always has a GL name");
  assert_eq!(out.batches[0].texture, Some((name, TextureSampling::Linear)));
  let draw = &out.mesh_draws[0];
  assert_eq!(draw.texture, Some((name, TextureSampling::Linear)));
  let texel = draw.texel.expect("a textured mesh draw carries its texel map");
  // The image's own map over the texture's size, in the path's space: the
  // translation of the draw is not in it.
  let corner = eval(&texel, 5.0, 5.0);
  assert!(near(corner.0, 0.0) && near(corner.1, 0.0), "{corner:?}");
  let far = eval(&texel, 25.0, 25.0);
  assert!(near(far.0, 1.0) && near(far.1, 1.0), "{far:?}");
  assert_eq!(draw.color, [1.0; 4]);
}

#[test]
fn a_rounded_clip_rides_on_the_mesh_draw() {
  let mut b = DisplayListBuilder::new(None);
  b.save().clip_rounded_rect(&rect(0.0, 0.0, 50.0, 50.0), &RoundingRadii::circular([5.0; 4]), ClipOperation::Intersect);
  b.draw_path(&square(0.0, 0.0, 10.0), &solid());
  b.restore();
  b.draw_path(&square(20.0, 0.0, 10.0), &solid());
  let out = walked(b);
  assert_eq!(out.batches.len(), 2);
  let rounded = out.mesh_draws[0].rounded.expect("the clipped path carries the rounded clip");
  assert_eq!(rounded.rect, Box2D::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 50.0)));
  assert_eq!(rounded.radii, [5.0; 4]);
  assert_eq!(out.mesh_draws[0].clip, [0.0, 0.0, 50.0, 50.0]);
  assert!(out.mesh_draws[1].rounded.is_none());
}

#[test]
fn stroke_and_fill_is_two_meshes_in_two_batches() {
  let mut paint = solid();
  paint.set_draw_style(DrawStyle::StrokeAndFill).set_stroke_width(2.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_path(&square(10.0, 10.0, 10.0), &paint);
  let out = walked(b);
  assert_eq!((out.meshes, out.tessellated, out.batches.len()), (2, 2, 2));
  // The stroke's outer half reaches past the fill.
  assert!(covers(&out, [9.5, 15.0]));
}

#[test]
fn quads_and_meshes_interleave_in_painters_order() {
  let mut b = DisplayListBuilder::new(None);
  b.save().clip_rect(&rect(0.0, 0.0, 20.0, 20.0), ClipOperation::Intersect);
  b.draw_path(&square(0.0, 0.0, 10.0), &solid());
  b.restore().save().clip_rect(&rect(30.0, 30.0, 20.0, 20.0), ClipOperation::Intersect);
  b.draw_rect(&rect(30.0, 30.0, 10.0, 10.0), &solid());
  b.restore().save().clip_rect(&rect(60.0, 0.0, 20.0, 20.0), ClipOperation::Intersect);
  b.draw_path(&square(60.0, 0.0, 10.0), &solid());
  b.restore();
  let out = walked(b);
  let programs: Vec<BatchProgram> = out.batches.iter().map(|b| b.program).collect();
  assert_eq!(programs, vec![BatchProgram::Mesh, BatchProgram::Fill, BatchProgram::Mesh]);
  assert_eq!((out.batches[1].start, out.batches[1].count), (0, 1), "the rect is instance 0");
  assert_eq!((out.batches[2].start, out.batches[2].count), (1, 1), "the second mesh draw follows the first");
  assert_eq!(out.scissor, Some(Box2D::new(Point2D::new(0, 0), Point2D::new(80, 50))));
  let r = report(&out);
  assert_eq!((r.instances, r.draw_calls, r.meshes), (1, 3, 2));
}

#[test]
fn a_stroke_without_width_records_nothing() {
  let mut paint = solid();
  paint.set_draw_style(DrawStyle::Stroke).set_stroke_width(0.0);
  let mut b = DisplayListBuilder::new(None);
  b.draw_path(&square(0.0, 0.0, 10.0), &paint);
  let list = b.build().expect("a builder always builds a list");
  // The log still has the op; nothing draws.
  assert_eq!(list.ops().len(), 1);
  let out = walk(&list, ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new());
  assert!(out.batches.is_empty() && out.mesh_draws.is_empty());
  assert_eq!(out.meshes, 0);
}

// The view's card flip about (`c`, `c`): rotate_y by `degrees`, then the
// perspective foreshorten at `distance`, as View::compose_user composes it.
fn card_flip(c: f32, degrees: f32, distance: f32) -> Matrix {
  let (s, co) = degrees.to_radians().sin_cos();
  #[rustfmt::skip]
  let ry = Matrix::new(
    co,  0.0, -s,  0.0,
    0.0, 1.0, 0.0, 0.0,
    s,   0.0, co,  0.0,
    0.0, 0.0, 0.0, 1.0,
  );
  Matrix::translation(-c, -c, 0.0).then(&ry).then(&Matrix::perspective(distance)).then(&Matrix::translation(c, c, 0.0))
}

#[test]
fn a_gradient_mesh_under_perspective_keeps_its_rows_over_the_path() {
  let flip = card_flip(TARGET / 2.0, 60.0, 200.0);
  let source = ColorSource::linear_gradient(
    Point::new(20.0, 0.0),
    Point::new(80.0, 0.0),
    &[Color::new_srgba(1.0, 0.0, 0.0, 1.0), Color::new_srgba(0.0, 0.0, 1.0, 1.0)],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  );
  let mut paint = solid();
  paint.set_color_source(&source);
  let mut b = DisplayListBuilder::new(None);
  b.transform(&flip).draw_path(&square(20.0, 20.0, 60.0), &paint);
  let out = walked(b);
  assert_eq!(out.batches.len(), 1);
  let draw = &out.mesh_draws[0];
  assert_eq!(draw.map, Projective::from_matrix(&flip));
  assert!(!draw.map.is_affine(), "the map carries the perspective row");
  let gradient = draw.gradient.as_ref().expect("the draw carries the gradient");
  assert_eq!(gradient.rows, [gradient.gradient.x_row, gradient.gradient.y_row]);
  // The kind and the tile reach the program's `uGradMeta` apart.
  assert_eq!((gradient.kind(), gradient.tile()), (GradientKind::Linear as i32 as f32, TILE_CLAMP));
  // A path point reads the gradient's t there with no divide: the GPU
  // interpolates the path-space point perspective-correct.
  for (x, t) in [(50.0, 0.5), (35.0, 0.25)] {
    let got = eval(&gradient.rows, x, 40.0).0;
    assert!(near(got, t), "path x {x}: t {got}, want {t}");
  }
}

#[test]
fn a_mesh_with_a_corner_behind_the_eye_is_drawn() {
  // A quarter turn under a close eye: w' = 1 + x / 10, behind the eye for
  // x below -10. The GPU clips the homogeneous vertices.
  let flip = card_flip(0.0, 90.0, 10.0);
  let mut b = DisplayListBuilder::new(None);
  b.transform(&flip).draw_path(&square(-300.0, 0.0, 250.0), &solid());
  let out = walked(b);
  assert_eq!((out.batches.len(), out.meshes), (1, 1), "{:?}", out.batches);
  assert_eq!(out.mesh_draws[0].clip, [0.0, 0.0, TARGET, TARGET], "not culled: the state's clip box");
}

#[test]
fn a_gpu_slot_is_kept_only_by_the_renderer_that_filled_it() {
  let mine = Arc::new(DeletionQueue::new());
  let dead = Arc::new(DeletionQueue::new());
  assert_eq!(gpu_slot(None, &mine), GpuSlot::Upload, "an empty slot uploads");
  assert_eq!(gpu_slot(Some(&mine.clone()), &mine), GpuSlot::Keep, "the same queue keeps its buffers");
  assert_eq!(gpu_slot(Some(&dead), &mine), GpuSlot::Replace, "another renderer's slot is replaced");
}

#[test]
fn a_dropped_mesh_returns_its_buffer_names_to_the_queue_that_made_them() {
  let queue = Arc::new(DeletionQueue::new());
  let name = |n: u32| glow::NativeBuffer(std::num::NonZeroU32::new(n).expect("a GL name is never zero"));
  let path = square(0.0, 0.0, 10.0);
  let mesh = tessellate(path.geometry(), FillType::NonZero, MeshStyle::Fill, 1.0).expect("a square tessellates");
  *mesh.gpu.lock().expect("fresh lock") = Some(GpuMesh { vertices: name(7), indices: name(8), queue: queue.clone() });
  drop(mesh);
  assert_eq!(queue.drain(), vec![Freed::Buffers(name(7), name(8))], "the drop pushed both names, deleting none");
  // A mesh never uploaded pushes nothing.
  let mesh = tessellate(path.geometry(), FillType::NonZero, MeshStyle::Fill, 1.0).expect("a square tessellates");
  drop(mesh);
  assert!(queue.drain().is_empty());
}
