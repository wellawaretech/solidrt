// The order a draw runs its batches in (renderer/schedule.rs): a layer that
// does not read the pixels beneath it drawn ahead of the frame it
// composites into, children before parents, a backdrop layer inline, an
// empty layer dropped whole, and the walk's order when drawing ahead is
// off; read off a walk with no GL context.

use crate::prelude::*;
use crate::renderer::ramps::RampTable;
use crate::renderer::schedule::{pack, schedule, Place, Placed, Step};
use crate::renderer::walk::{walk, BatchProgram, Walked};
use crate::renderer::{layer_passes, report};
use crate::{ClipOperation, Color, DisplayListBuilder, ISize, ImageFilter, Matrix, Paint, Point, Rect, Size, TileMode};

// The target every walk here draws into, in device pixels.
const TARGET: f32 = 100.0;

// The group alpha the layers here fade by.
const HALF: f32 = 0.5;

// The backdrop's blur sigma: any blur makes the backdrop a layer.
const SIGMA: f32 = 2.0;

// The rotation the empty case's stencil clip is under, in degrees: anything
// that keeps a rect off the axes, so the clip is a stencil clip.
const DEGREES: f32 = 30.0;

use BatchProgram::{Fill, LayerBegin, LayerEnd};
use Place::Own;
use Step::{AtlasBegin, AtlasEnd, Batch, Close, Composite, Open, Target};

// The atlas the packing cases share: the target's size, as the executor
// takes it.
const ATLAS: (i32, i32) = (TARGET as i32, TARGET as i32);

// A region of the atlas numbered `atlas` at `offset`.
fn region(atlas: usize, offset: (i32, i32)) -> Place {
  Place::Region { atlas, offset }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn paint(alpha: f32) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, alpha));
  paint
}

fn walked(b: DisplayListBuilder) -> Walked {
  let list = b.build().expect("a builder always builds a list");
  walk(&list, ISize::new(TARGET as i64, TARGET as i64), &mut RampTable::new())
}

fn programs(out: &Walked) -> Vec<BatchProgram> {
  out.batches.iter().map(|b| b.program).collect()
}

// The steps with every layer in storage of its own (no atlas).
fn steps(out: &Walked, ahead: bool) -> Vec<Step> {
  schedule(&out.batches, &out.layers, ahead, None)
}

// The steps with the drawn-ahead layers packed into atlases of ATLAS.
fn atlased(out: &Walked) -> Vec<Step> {
  schedule(&out.batches, &out.layers, true, Some(ATLAS))
}

// A group fading two overlapping rects at (x, y): overlapping, so its alpha
// cannot fold and it draws as a layer.
fn group(b: &mut DisplayListBuilder, x: f32, y: f32) {
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.draw_rect(&rect(x, y, 10.0, 10.0), &paint(1.0)).draw_rect(&rect(x + 5.0, y + 5.0, 10.0, 10.0), &paint(1.0));
  b.restore();
}

// A rect, a group, a rect and a group: the target's pass draws its two rects
// and the two composites; the groups draw before it.
fn two_groups() -> Walked {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &paint(1.0));
  group(&mut b, 10.0, 10.0);
  b.draw_rect(&rect(50.0, 0.0, 5.0, 5.0), &paint(1.0));
  group(&mut b, 60.0, 60.0);
  walked(b)
}

#[test]
fn layers_draw_ahead_of_the_target_and_composite_in_place() {
  let out = two_groups();
  assert_eq!(programs(&out), [Fill, LayerBegin, Fill, LayerEnd, Fill, LayerBegin, Fill, LayerEnd]);
  assert_eq!(
    steps(&out, true),
    [
      Open(1, Own),
      Batch(2),
      Close(3, Own),
      Open(5, Own),
      Batch(6),
      Close(7, Own),
      Target,
      Batch(0),
      Composite(3),
      Batch(4),
      Composite(7),
    ]
  );
  let r = report(&out);
  assert_eq!((r.layers, r.layers_ahead), (2, 2));
  assert_eq!(layer_passes(&steps(&out, true)), 2, "a pass per layer without an atlas");
}

#[test]
fn the_layers_of_one_level_share_an_atlas_pass() {
  let out = two_groups();
  // Both groups are 15 by 15 device pixels: side by side in one atlas.
  let (first, second) = (region(0, (0, 0)), region(0, (15, 0)));
  assert_eq!(
    atlased(&out),
    [
      AtlasBegin { atlas: 0, size: ATLAS },
      Open(1, first),
      Batch(2),
      Close(3, first),
      Open(5, second),
      Batch(6),
      Close(7, second),
      AtlasEnd(0),
      Target,
      Batch(0),
      Composite(3),
      Batch(4),
      Composite(7),
    ]
  );
  assert_eq!(layer_passes(&atlased(&out)), 1);
}

#[test]
fn without_drawing_ahead_the_walk_order_stands() {
  let out = two_groups();
  assert_eq!(
    steps(&out, false),
    [
      Target,
      Batch(0),
      Open(1, Own),
      Batch(2),
      Close(3, Own),
      Composite(3),
      Batch(4),
      Open(5, Own),
      Batch(6),
      Close(7, Own),
      Composite(7),
    ]
  );
}

#[test]
fn a_layer_inside_a_layer_draws_before_it() {
  let mut b = DisplayListBuilder::new(None);
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  group(&mut b, 10.0, 10.0);
  b.draw_rect(&rect(60.0, 60.0, 10.0, 10.0), &paint(1.0));
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), [LayerBegin, LayerBegin, Fill, LayerEnd, Fill, LayerEnd]);
  assert_eq!(
    steps(&out, true),
    [Open(1, Own), Batch(2), Close(3, Own), Open(0, Own), Composite(3), Batch(4), Close(5, Own), Target, Composite(5)]
  );
  // Atlased, the inner level's atlas is drawn whole before the outer
  // level's, which samples it: each level its own atlas.
  let (inner, outer) = (region(0, (0, 0)), region(1, (0, 0)));
  assert_eq!(
    atlased(&out),
    [
      AtlasBegin { atlas: 0, size: ATLAS },
      Open(1, inner),
      Batch(2),
      Close(3, inner),
      AtlasEnd(0),
      AtlasBegin { atlas: 1, size: ATLAS },
      Open(0, outer),
      Composite(3),
      Batch(4),
      Close(5, outer),
      AtlasEnd(1),
      Target,
      Composite(5),
    ]
  );
  assert_eq!(layer_passes(&atlased(&out)), 2);
}

#[test]
fn a_blurred_layer_keeps_storage_of_its_own_beside_the_atlas() {
  let mut b = DisplayListBuilder::new(None);
  group(&mut b, 10.0, 10.0);
  let blur = ImageFilter::gaussian_blur(SIGMA, SIGMA, TileMode::Decal);
  let mut blurred = paint(1.0);
  blurred.set_image_filter(&blur);
  b.save_layer(&Rect::zero(), Some(&blurred));
  b.draw_rect(&rect(50.0, 50.0, 10.0, 10.0), &paint(1.0));
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), [LayerBegin, Fill, LayerEnd, LayerBegin, Fill, LayerEnd]);
  assert!(out.layers[out.batches[3].start].blur.is_some());
  // The blurred layer first, in its own storage; then the group's atlas.
  let first = region(0, (0, 0));
  assert_eq!(
    atlased(&out),
    [
      Open(3, Own),
      Batch(4),
      Close(5, Own),
      AtlasBegin { atlas: 0, size: ATLAS },
      Open(0, first),
      Batch(1),
      Close(2, first),
      AtlasEnd(0),
      Target,
      Composite(2),
      Composite(5),
    ]
  );
  assert_eq!(layer_passes(&atlased(&out)), 2);
}

#[test]
fn packing_fills_rows_then_atlases() {
  // Three regions along a row of 100: 40, 40, then 40 does not fit beside
  // them and starts a row above, as tall as the tallest below (30).
  let placed = pack(&[(40, 20), (40, 30), (40, 10)], (100, 100));
  assert_eq!(
    placed,
    [Placed { atlas: 0, offset: (0, 0) }, Placed { atlas: 0, offset: (40, 0) }, Placed { atlas: 0, offset: (0, 30) }]
  );
  // Rows that do not fit above move to the next atlas.
  let placed = pack(&[(100, 60), (100, 60), (50, 50)], (100, 100));
  assert_eq!(
    placed,
    [Placed { atlas: 0, offset: (0, 0) }, Placed { atlas: 1, offset: (0, 0) }, Placed { atlas: 2, offset: (0, 0) }]
  );
  // A region as large as the atlas takes it alone.
  assert_eq!(
    pack(&[(100, 100), (10, 10)], (100, 100)),
    [Placed { atlas: 0, offset: (0, 0) }, Placed { atlas: 1, offset: (0, 0) }]
  );
}

#[test]
fn a_backdrop_layer_draws_inline_and_a_group_inside_it_ahead() {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 50.0, 50.0), &paint(1.0));
  let blur = ImageFilter::gaussian_blur(SIGMA, SIGMA, TileMode::Clamp);
  b.save_backdrop_layer(&rect(5.0, 5.0, 40.0, 40.0), None, Some(&blur));
  group(&mut b, 10.0, 10.0);
  b.restore();
  let out = walked(b);
  assert_eq!(programs(&out), [Fill, LayerBegin, LayerBegin, Fill, LayerEnd, LayerEnd]);
  assert!(out.layers[out.batches[1].start].backdrop);
  assert_eq!(
    steps(&out, true),
    [Open(2, Own), Batch(3), Close(4, Own), Target, Batch(0), Open(1, Own), Composite(4), Close(5, Own), Composite(5)]
  );
  let r = report(&out);
  assert_eq!((r.layers, r.layers_ahead), (2, 1), "the backdrop reads the target: it draws in place");
  // The group inside the backdrop is at the second nesting level: its atlas
  // is drawn ahead, and the backdrop stays inline, a pass of its own.
  let inner = region(0, (0, 0));
  assert_eq!(
    atlased(&out),
    [
      AtlasBegin { atlas: 0, size: ATLAS },
      Open(2, inner),
      Batch(3),
      Close(4, inner),
      AtlasEnd(0),
      Target,
      Batch(0),
      Open(1, Own),
      Composite(4),
      Close(5, Own),
      Composite(5)
    ]
  );
  assert_eq!(layer_passes(&atlased(&out)), 2);
}

#[test]
fn an_empty_layer_drops_out_with_its_content() {
  // A stencil clip inside keeps the layer from folding; the rect lies
  // outside it, so nothing of the layer shows.
  let c = TARGET / 2.0;
  let rotated = Matrix::translation(-c, -c, 0.0)
    .then(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(DEGREES)))
    .then(&Matrix::translation(c, c, 0.0));
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 5.0, 5.0), &paint(1.0));
  b.save_layer(&Rect::zero(), Some(&paint(HALF)));
  b.save().transform(&rotated).clip_rect(&rect(30.0, 30.0, 20.0, 20.0), ClipOperation::Intersect);
  b.transform(&rotated.inverse().expect("a rotation inverts"));
  b.draw_rect(&rect(80.0, 80.0, 10.0, 10.0), &paint(1.0));
  b.restore();
  b.restore();
  let out = walked(b);
  assert!(out.layers[0].rect.is_empty());
  assert_eq!(steps(&out, true), [Target, Batch(0)]);
}
