use crate::context::Mirror;
use crate::gpu::{
  project_box, BoxDamage, CellBox, CellIndex, DamageRects, ListDamage, OrderView, PxRect, RecordExtent, TargetDirt,
  CELL_BOX_FLOATS, MAX_DAMAGE_RECTS, MAX_DAMAGE_SLOTS,
};
use crate::raster::DirtySet;
use crate::spatial::Lattice;

// The target and view size of the projection cases, in pixels and world
// units alike (one unit per pixel unless a case says otherwise).
const SIDE: f32 = 100.0;

fn px(x: i32, y: i32, w: u32, h: u32) -> PxRect {
  PxRect { x, y, w, h }
}

// The y-down orthographic view of a `w` x `h` world rectangle at (x, y),
// as the pixel contract has it: world y = y lands at clip y -1, the
// target's top row.
fn view_down(x: f32, y: f32, w: f32, h: f32) -> OrderView {
  let mut m = [0.0f32; 16];
  m[0] = 2.0 / w;
  m[5] = 2.0 / h;
  m[10] = -1.0;
  m[12] = -2.0 * x / w - 1.0;
  m[13] = -2.0 * y / h - 1.0;
  m[15] = 1.0;
  OrderView { clip: m }
}

// A GL perspective (near 1, far 10000, focal 1.5) with its eye at the
// origin looking down -z.
fn perspective() -> OrderView {
  let (n, f) = (1.0f32, 10000.0f32);
  let mut m = [0.0f32; 16];
  m[0] = 1.5;
  m[5] = 1.5;
  m[10] = -(f + n) / (f - n);
  m[11] = -1.0;
  m[14] = -2.0 * f * n / (f - n);
  OrderView { clip: m }
}

// The box from (x0, y0) to (x1, y1) in the z = 0 plane, reaching `reach`
// screen pixels beyond itself.
fn rect_box(x0: f32, y0: f32, x1: f32, y1: f32, reach: f32) -> CellBox {
  CellBox { bounds: [x0, y0, 0.0, x1, y1, 0.0], px: reach }
}

fn column(boxes: &[CellBox]) -> Vec<f32> {
  boxes.iter().flat_map(|b| b.floats()).collect()
}

fn lattice() -> Lattice {
  Lattice { origin: [0.0; 3], cell: [SIDE as f64, SIDE as f64, 0.0] }
}

#[test]
fn a_rectangle_set_merges_what_overlaps_or_touches() {
  let mut set = DamageRects::new();
  set.add(px(0, 0, 10, 10));
  set.add(px(5, 5, 10, 10));
  assert_eq!(set.rects(), &[px(0, 0, 15, 15)], "overlapping rectangles merge");
  set.add(px(15, 0, 5, 5));
  assert_eq!(set.rects(), &[px(0, 0, 20, 15)], "a rectangle touching an edge merges");
  set.add(px(40, 40, 5, 5));
  assert_eq!(set.rects().len(), 2, "a rectangle apart stays apart");
  // One rectangle bridging both merges the three.
  set.add(px(18, 13, 25, 30));
  assert_eq!(set.rects(), &[px(0, 0, 45, 45)]);
  set.add(px(3, 3, 0, 9));
  assert_eq!(set.rects().len(), 1, "an empty rectangle is dropped");
}

#[test]
fn past_the_cap_the_rectangles_collapse_to_their_bound() {
  let mut set = DamageRects::new();
  for i in 0..MAX_DAMAGE_RECTS as i32 {
    set.add(px(i * 20, 0, 5, 5));
  }
  assert_eq!(set.rects().len(), MAX_DAMAGE_RECTS, "up to the cap each rectangle is kept");
  set.add(px(0, 50, 5, 5));
  assert_eq!(set.rects(), &[px(0, 0, (MAX_DAMAGE_RECTS as u32 - 1) * 20 + 5, 55)]);
}

#[test]
fn a_rectangle_set_clips_to_its_owner_and_says_when_it_covers_it() {
  let mut set = DamageRects::new();
  set.add(px(-5, -5, 10, 10));
  set.add(px(50, 50, 10, 10));
  set.add(px(200, 0, 10, 10));
  set.clip(54, 54);
  assert_eq!(set.rects(), &[px(0, 0, 5, 5), px(50, 50, 4, 4)], "cut to the owner, the outside one dropped");
  assert_eq!(set.area(), 41);
  // Coverage is strictly above half of the owner.
  let mut half = DamageRects::new();
  half.add(px(0, 0, 10, 5));
  assert!(!half.covers(10, 10), "exactly half does not cover");
  half.add(px(0, 5, 1, 1));
  assert!(half.covers(10, 10), "over half covers");
}

#[test]
fn a_box_projects_into_the_target_y_down() {
  let view = view_down(0.0, 0.0, SIDE, SIDE);
  let target = [SIDE, SIDE];
  // A box near the top-left lands top-left, grown by the edge pixel: a
  // vertically mirrored projection would put it at y 79.
  assert_eq!(
    project_box(&view, &rect_box(10.0, 10.0, 20.0, 20.0, 0.0), target, target),
    BoxDamage::Rect(px(9, 9, 12, 12))
  );
  // An asymmetric one: near the top edge only.
  assert_eq!(
    project_box(&view, &rect_box(40.0, 2.0, 60.0, 6.0, 0.0), target, target),
    BoxDamage::Rect(px(39, 1, 22, 6))
  );
  // Fractional bounds round outward.
  assert_eq!(
    project_box(&view, &rect_box(10.5, 10.5, 19.5, 19.5, 0.0), target, target),
    BoxDamage::Rect(px(9, 9, 12, 12))
  );
  // Partly outside: cut to the target.
  assert_eq!(
    project_box(&view, &rect_box(-10.0, 90.0, 10.0, 110.0, 0.0), target, target),
    BoxDamage::Rect(px(0, 89, 11, 11))
  );
  // Wholly outside, and empty: nothing.
  assert_eq!(project_box(&view, &rect_box(500.0, 500.0, 510.0, 510.0, 0.0), target, target), BoxDamage::Outside);
  assert_eq!(project_box(&view, &CellBox::EMPTY, target, target), BoxDamage::Outside);
}

#[test]
fn the_screen_reach_scales_under_an_oversample() {
  let view = view_down(0.0, 0.0, SIDE, SIDE);
  // Reach 5 viewport pixels plus the edge pixel at one target pixel per
  // viewport pixel.
  let b = rect_box(10.0, 10.0, 20.0, 20.0, 5.0);
  assert_eq!(project_box(&view, &b, [SIDE, SIDE], [SIDE, SIDE]), BoxDamage::Rect(px(4, 4, 22, 22)));
  // The target renders at twice the viewport: the box covers 20..40, its
  // reach is 10 target pixels.
  let target = [SIDE * 2.0, SIDE * 2.0];
  assert_eq!(project_box(&view, &b, [SIDE, SIDE], target), BoxDamage::Rect(px(9, 9, 42, 42)));
  // Per axis: twice as wide only.
  assert_eq!(project_box(&view, &b, [SIDE, SIDE], [SIDE * 2.0, SIDE]), BoxDamage::Rect(px(9, 4, 42, 22)));
}

#[test]
fn a_box_without_a_bound_says_so() {
  let view = view_down(0.0, 0.0, SIDE, SIDE);
  let target = [SIDE, SIDE];
  let unbounded = rect_box(0.0, 0.0, f32::INFINITY, 10.0, 0.0);
  assert_eq!(project_box(&view, &unbounded, target, target), BoxDamage::Unbounded, "a corner that is not finite");
  let unreached = rect_box(0.0, 0.0, 10.0, 10.0, f32::NAN);
  assert_eq!(project_box(&view, &unreached, target, target), BoxDamage::Unbounded, "a reach that is not finite");
  // Under a perspective view: a box in front projects, one at or behind
  // the eye has no bound.
  let eye = perspective();
  let front = CellBox { bounds: [-1.0, -1.0, -10.0, 1.0, 1.0, -9.0], px: 0.0 };
  assert!(matches!(project_box(&eye, &front, target, target), BoxDamage::Rect(_)));
  let behind = CellBox { bounds: [-1.0, -1.0, -10.0, 1.0, 1.0, 5.0], px: 0.0 };
  assert_eq!(project_box(&eye, &behind, target, target), BoxDamage::Unbounded);
}

#[test]
fn raster_dirt_is_full_or_rectangles_and_full_wins() {
  let mut dirt = DirtySet::new();
  assert!(dirt.is_empty());
  dirt.damage(1, &[px(0, 0, 4, 4)]);
  assert!(dirt.contains(&1) && !dirt.is_full(&1), "rectangles on a clean id");
  assert_eq!(dirt.rects(&1).map(DamageRects::rects), Some(&[px(0, 0, 4, 4)][..]));
  dirt.damage(1, &[px(4, 0, 4, 4), px(20, 20, 2, 2)]);
  assert_eq!(dirt.rects(&1).map(DamageRects::rects), Some(&[px(0, 0, 8, 4), px(20, 20, 2, 2)][..]), "merged");
  assert!(dirt.has_rects());
  dirt.insert(1);
  assert!(dirt.is_full(&1) && dirt.rects(&1).is_none(), "full drops the rectangles");
  dirt.damage(1, &[px(0, 0, 4, 4)]);
  assert!(dirt.is_full(&1), "rectangles never narrow a full dirt");
  assert!(!dirt.has_rects());
  dirt.damage(2, &[px(0, 0, 0, 4)]);
  assert!(!dirt.contains(&2), "a clean id given only empty rectangles stays clean");
  dirt.extend([3, 4]);
  dirt.damage(5, &[px(1, 1, 1, 1)]);
  assert_eq!(dirt.ids(), [1, 3, 4, 5].into_iter().collect::<std::collections::HashSet<u64>>());
  dirt.remove(&3);
  assert!(!dirt.contains(&3) && dirt.contains(&4));
  dirt.clear();
  assert!(dirt.is_empty() && dirt.ids().is_empty());
}

// Records of the staged-box mirror cases: two floats, unread.
const STAGED_STRIDE: usize = 8;

// The floats a sink stages per record of `STAGED_STRIDE` bytes.
const STAGED_FLOATS: usize = STAGED_STRIDE / 4;

fn old_box(mirror: &Mirror, slot: u32) -> Option<CellBox> {
  mirror.old_box(slot)
}

#[test]
fn the_mirror_keeps_a_staged_box_as_of_the_last_point() {
  let a = [rect_box(0.0, 0.0, 1.0, 1.0, 0.0), rect_box(2.0, 2.0, 3.0, 3.0, 0.0), rect_box(4.0, 4.0, 5.0, 5.0, 0.0)];
  let mut mirror = Mirror::for_list(vec![0u8; 3 * STAGED_STRIDE], STAGED_STRIDE, None);
  mirror.write_boxes(0, 3 * STAGED_FLOATS, &column(&a));
  assert_eq!(old_box(&mirror, 0), Some(CellBox::EMPTY), "a slot with no box before had the empty one");
  mirror.clear_changed();
  assert!(mirror.changed_slots().is_empty() && old_box(&mirror, 0).is_none(), "cleared at the point");
  // Two writes between points: the first mark keeps the box of the point.
  let mut b = a;
  b[1] = rect_box(20.0, 20.0, 30.0, 30.0, 0.0);
  mirror.write_boxes(STAGED_FLOATS, 2 * STAGED_FLOATS, &column(&b));
  b[1] = rect_box(40.0, 40.0, 50.0, 50.0, 0.0);
  mirror.write_boxes(STAGED_FLOATS, 2 * STAGED_FLOATS, &column(&b));
  assert_eq!(mirror.changed_slots(), &[1]);
  assert_eq!(old_box(&mirror, 1), Some(a[1]), "first mark wins");
  assert_eq!(CellBox::read(mirror.box_column(), 1), b[1], "the column holds the newest box");
  // A record written as it was, and a box written as it was, mark nothing.
  mirror.clear_changed();
  mirror.write_bytes(0, &[0u8; STAGED_STRIDE]);
  mirror.write_boxes(0, STAGED_FLOATS, &column(&b));
  assert!(mirror.changed_slots().is_empty());
  // Past the point the newest box is the old one.
  b[1] = rect_box(60.0, 60.0, 70.0, 70.0, 0.0);
  mirror.write_boxes(STAGED_FLOATS, 2 * STAGED_FLOATS, &column(&b));
  assert_eq!(old_box(&mirror, 1), Some(rect_box(40.0, 40.0, 50.0, 50.0, 0.0)));
  // A slot the column loses is marked with the box it had.
  mirror.clear_changed();
  mirror.write_boxes(0, 2 * STAGED_FLOATS, &column(&b[..2]));
  assert_eq!(old_box(&mirror, 2), Some(a[2]));
  assert_eq!(mirror.box_column().len(), 2 * CELL_BOX_FLOATS);
}

// A record-layer record: center (vec2) and size (vec2).
const EXTENT_STRIDE: usize = 16;

fn extent_record(cx: f32, cy: f32, w: f32, h: f32) -> Vec<u8> {
  [cx, cy, w, h].iter().flat_map(|f| f.to_ne_bytes()).collect()
}

#[test]
fn the_mirror_keeps_a_record_box_as_of_the_last_point() {
  let extent = RecordExtent { position: 0, size: 8, rotation: None, min_screen_px: None };
  let mut data = extent_record(10.0, 10.0, 4.0, 4.0);
  data.extend(extent_record(30.0, 30.0, 4.0, 4.0));
  let mut mirror = Mirror::for_list(data, EXTENT_STRIDE, Some(extent));
  mirror.refresh_boxes();
  mirror.clear_changed();
  let at_point = rect_box(8.0, 8.0, 12.0, 12.0, 0.0);
  assert_eq!(CellBox::read(mirror.box_column(), 0), at_point);
  // Two moves of slot 0 between points: the box is re-read from the
  // records only at the next point, and the old one is the point's.
  mirror.write_bytes(0, &extent_record(50.0, 50.0, 4.0, 4.0));
  mirror.write_bytes(0, &extent_record(70.0, 70.0, 4.0, 4.0));
  assert_eq!(old_box(&mirror, 0), Some(at_point), "first mark wins");
  mirror.refresh_boxes();
  assert_eq!(CellBox::read(mirror.box_column(), 0), rect_box(68.0, 68.0, 72.0, 72.0, 0.0));
  assert_eq!(old_box(&mirror, 0), Some(at_point), "the re-read keeps the old box");
  assert_eq!(old_box(&mirror, 1), None, "an unwritten slot is not marked");
  mirror.clear_changed();
  assert!(mirror.changed_slots().is_empty() && old_box(&mirror, 0).is_none());
}

// One list of the derivation cases: four records in a 100 x 100 view of a
// 100 x 100 target, built over all four, with the given changes.
struct Case {
  index: CellIndex,
  view: OrderView,
  boxes: Vec<f32>,
  old_slots: Vec<u32>,
  old_boxes: Vec<f32>,
  touched: Vec<u32>,
  built: Option<usize>,
  population: usize,
  stale: bool,
}

impl Case {
  fn new(boxes: &[CellBox]) -> Case {
    Case {
      index: CellIndex::new(lattice()),
      view: view_down(0.0, 0.0, SIDE, SIDE),
      boxes: column(boxes),
      old_slots: Vec::new(),
      old_boxes: Vec::new(),
      touched: Vec::new(),
      built: Some(boxes.len()),
      population: boxes.len(),
      stale: false,
    }
  }

  // Slot `slot` moved here from `old`: its new box is in the column, the
  // old one captured.
  fn moved(mut self, slot: usize, old: CellBox, new: CellBox) -> Case {
    self.boxes[slot * CELL_BOX_FLOATS..(slot + 1) * CELL_BOX_FLOATS].copy_from_slice(&new.floats());
    self.old_slots.push(slot as u32);
    self.old_boxes.extend_from_slice(&old.floats());
    self.touched.push(slot as u32);
    self.touched.sort_unstable();
    self
  }

  fn dirt(&self) -> TargetDirt {
    self.dirt_with(Some(&self.index), Some(&self.view), Some([SIDE, SIDE]))
  }

  fn dirt_with(&self, index: Option<&CellIndex>, view: Option<&OrderView>, target: Option<[f32; 2]>) -> TargetDirt {
    ListDamage {
      index,
      view,
      stale: self.stale,
      built: self.built,
      population: self.population,
      touched: &self.touched,
      boxes: &self.boxes,
      old_box: &|s| self.old_slots.iter().position(|&o| o == s).map(|i| CellBox::read(&self.old_boxes, i)),
      viewport: [SIDE, SIDE],
      target,
    }
    .dirt()
  }
}

fn four() -> [CellBox; 4] {
  [
    rect_box(10.0, 10.0, 20.0, 20.0, 0.0),
    rect_box(30.0, 10.0, 40.0, 20.0, 0.0),
    rect_box(10.0, 60.0, 20.0, 70.0, 0.0),
    rect_box(60.0, 60.0, 70.0, 70.0, 0.0),
  ]
}

#[test]
fn a_move_damages_the_old_place_and_the_new() {
  let boxes = four();
  let case = Case::new(&boxes).moved(1, boxes[1], rect_box(30.0, 40.0, 40.0, 50.0, 0.0));
  assert_eq!(case.dirt(), TargetDirt::Rects(vec![px(29, 9, 12, 12), px(29, 39, 12, 12)]));
  // The cell index's margin grows both, as culling reads them.
  let mut grown = case;
  grown.index = CellIndex::new(lattice()).with_margin(2.0);
  assert_eq!(grown.dirt(), TargetDirt::Rects(vec![px(27, 7, 16, 16), px(27, 37, 16, 16)]));
}

#[test]
fn a_hide_damages_the_old_place_only() {
  let boxes = four();
  let case = Case::new(&boxes).moved(2, boxes[2], CellBox::EMPTY);
  assert_eq!(case.dirt(), TargetDirt::Rects(vec![px(9, 59, 12, 12)]));
}

#[test]
fn a_key_change_damages_the_record_where_it_is() {
  // A write to the key buffer alone: the box column's mirror marked
  // nothing, so the slot's box now is its box at the point, and the
  // rebuild that re-sorts it damages that box.
  let boxes = four();
  let mut case = Case::new(&boxes);
  case.touched = vec![3];
  assert_eq!(case.dirt(), TargetDirt::Rects(vec![px(59, 59, 12, 12)]));
}

#[test]
fn a_slot_joining_or_leaving_the_view_damages_its_box_in_view() {
  let boxes = four();
  let off = rect_box(300.0, 300.0, 310.0, 310.0, 0.0);
  let join = Case::new(&boxes).moved(0, off, boxes[0]);
  assert_eq!(join.dirt(), TargetDirt::Rects(vec![px(9, 9, 12, 12)]));
  let leave = Case::new(&boxes).moved(0, boxes[0], off);
  assert_eq!(leave.dirt(), TargetDirt::Rects(vec![px(9, 9, 12, 12)]));
  let unseen = Case::new(&boxes).moved(0, off, rect_box(400.0, 400.0, 410.0, 410.0, 0.0));
  assert_eq!(unseen.dirt(), TargetDirt::Rects(Vec::new()), "a move wholly off the view damages nothing");
}

#[test]
fn a_population_change_damages_the_slots_it_adds_or_removes() {
  let boxes = four();
  let mut shrunk = Case::new(&boxes);
  shrunk.population = 3;
  assert_eq!(shrunk.dirt(), TargetDirt::Rects(vec![px(59, 59, 12, 12)]), "slot 3 left the population");
  let mut grown = Case::new(&boxes);
  grown.built = Some(3);
  assert_eq!(grown.dirt(), TargetDirt::Rects(vec![px(59, 59, 12, 12)]), "slot 3 joined the population");
  // A touched slot past both populations is never drawn.
  let mut past = Case::new(&boxes);
  past.population = 3;
  past.built = Some(3);
  past.touched = vec![3];
  assert_eq!(past.dirt(), TargetDirt::Rects(Vec::new()));
}

#[test]
fn every_fallback_derives_full_dirt() {
  let boxes = four();
  let case = Case::new(&boxes).moved(1, boxes[1], rect_box(30.0, 40.0, 40.0, 50.0, 0.0));
  assert!(matches!(case.dirt(), TargetDirt::Rects(_)));
  assert_eq!(case.dirt_with(None, Some(&case.view), Some([SIDE, SIDE])), TargetDirt::Full, "a list that does not cull");
  assert_eq!(case.dirt_with(Some(&case.index), None, Some([SIDE, SIDE])), TargetDirt::Full, "no view");
  assert_eq!(case.dirt_with(Some(&case.index), Some(&case.view), None), TargetDirt::Full, "no target size");
  let stale = Case { stale: true, ..Case::new(&boxes).moved(1, boxes[1], boxes[0]) };
  assert_eq!(stale.dirt(), TargetDirt::Full, "a stale list");
  let first = Case { built: None, ..Case::new(&boxes) };
  assert_eq!(first.dirt(), TargetDirt::Full, "the first build");
  // One slot past the bulk cap.
  let many: Vec<CellBox> = (0..=MAX_DAMAGE_SLOTS).map(|i| rect_box(i as f32, 0.0, i as f32 + 1.0, 1.0, 0.0)).collect();
  let mut bulk = Case::new(&many);
  bulk.touched = (0..many.len() as u32).collect();
  assert_eq!(bulk.dirt(), TargetDirt::Full, "more than MAX_DAMAGE_SLOTS slots");
  bulk.touched.pop();
  assert!(matches!(bulk.dirt(), TargetDirt::Rects(_)), "MAX_DAMAGE_SLOTS slots still derive rectangles");
  // A box at the eye under a perspective view.
  let near = CellBox { bounds: [-1.0, -1.0, -10.0, 1.0, 1.0, 5.0], px: 0.0 };
  let mut eye = Case::new(&[near]);
  eye.view = perspective();
  eye.touched = vec![0];
  assert_eq!(eye.dirt(), TargetDirt::Full, "a box without a bound");
}

// The previous `Mirror::write_bytes` loop, kept as the reference the
// current one must match in bytes, held length and marks.
fn reference_write(
  data: &mut Vec<u8>,
  held: &mut usize,
  stride: usize,
  offset: usize,
  new: &[u8],
  marks: &mut Vec<u32>,
) {
  let end = offset + new.len();
  let before = *held;
  if data.len() < end {
    data.resize(end, 0);
  }
  let mut at = offset;
  while at < end {
    let slot = at / stride;
    let next = ((slot + 1) * stride).min(end);
    let src = &new[at - offset..next - offset];
    if at >= before || data[at..next] != *src {
      data[at..next].copy_from_slice(src);
      if !marks.contains(&(slot as u32)) {
        marks.push(slot as u32);
      }
    }
    at = next;
  }
  *held = (*held).max(end);
}

#[test]
fn a_record_write_marks_and_stores_as_before() {
  // Three 8-byte records held; the cases: a partial record, a write that
  // spans two partial records, an unchanged rewrite, a write past `held`.
  let start: Vec<u8> = (0..3 * STAGED_STRIDE as u8).collect();
  let cases: [(usize, Vec<u8>); 5] = [
    (2, vec![200, 201, 202]),
    (6, vec![210, 211, 212, 213]),
    (8, (8..16).collect()),
    (20, vec![220; 12]),
    (0, (0..8).collect()),
  ];
  let mut mirror = Mirror::for_list(start.clone(), STAGED_STRIDE, None);
  mirror.clear_changed();
  let (mut data, mut held, mut marks) = (start, 3 * STAGED_STRIDE, Vec::new());
  for (offset, bytes) in &cases {
    mirror.write_bytes(*offset, bytes);
    reference_write(&mut data, &mut held, STAGED_STRIDE, *offset, bytes, &mut marks);
    let (now, now_held) = mirror.record_bytes();
    assert_eq!(now, &data[..], "bytes after a write at {offset}");
    assert_eq!(now_held, held, "held after a write at {offset}");
    assert_eq!(mirror.changed_slots(), &marks[..], "marks after a write at {offset}");
  }
  assert_eq!(marks, vec![0, 1, 2, 3], "the unchanged rewrites marked nothing of their own");
}

// The list cases' buffers: records [cx, cy, w, h, key] in buffer 0 (the
// cull boxes and the key), one colour float per record in buffer 1.
const LIST_RECORD: usize = 20;
const LIST_KEY_BYTE: usize = 16;
const LIST_COLOUR: usize = 4;
const BOX_BUFFER: u64 = 1;
const COLOUR_BUFFER: u64 = 2;
// The list cases' target and view side in pixels (one world unit each),
// and the cell side.
const LIST_SIDE: f32 = 64.0;
const LIST_CELL: f64 = 16.0;
// The floats of one list record.
const LIST_FLOATS: usize = LIST_RECORD / 4;

fn list_record(cx: f32, cy: f32, w: f32, h: f32, key: f32) -> [f32; LIST_FLOATS] {
  [cx, cy, w, h, key]
}

fn floats_bytes(floats: &[f32]) -> Vec<u8> {
  floats.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

// A culling list over the two buffers, keyed by `key` when given, its
// boxes read off the records (`extent`) or staged.
fn test_list(population: usize, capacity: usize, keyed: bool, extent: bool) -> crate::context::ListEntry {
  let mut strides = [0usize; crate::gpu::MAX_BUFFERS];
  strides[0] = LIST_RECORD;
  strides[1] = LIST_COLOUR;
  let mut sources = [0u64; crate::gpu::MAX_BUFFERS];
  sources[0] = BOX_BUFFER;
  sources[1] = COLOUR_BUFFER;
  let key = keyed.then_some(crate::gpu::OrderKey::Field { offset: LIST_KEY_BYTE });
  let extent = extent.then_some(RecordExtent { position: 0, size: 8, rotation: None, min_screen_px: None });
  let lattice = Lattice { origin: [0.0; 3], cell: [LIST_CELL, LIST_CELL, 0.0] };
  let mut list = crate::context::ListEntry::for_test(key, 0, strides, sources, lattice, extent, population, capacity);
  list.set_view(view_down(0.0, 0.0, LIST_SIDE, LIST_SIDE)).expect("an orthographic view has a direction");
  list
}

// The box a staged record has: its rectangle in the z = 0 plane.
fn staged_box(r: &[f32]) -> CellBox {
  if !(r[2] > 0.0 && r[3] > 0.0) {
    return CellBox::EMPTY;
  }
  rect_box(r[0] - r[2] * 0.5, r[1] - r[3] * 0.5, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5, 0.0)
}

type Lists = std::collections::HashMap<(u64, u64), crate::context::ListEntry>;
type Mirrors = std::collections::HashMap<u64, Mirror>;

// The key of the one list of the list cases.
const LIST_KEY: (u64, u64) = (1, 1);

fn point(lists: &mut Lists, mirrors: &mut Mirrors) -> Option<crate::context::Refresh> {
  let mut scratch = crate::gpu::OrderScratch::default();
  let target = Some([LIST_SIDE, LIST_SIDE]);
  crate::context::rebuild_point(lists, mirrors, &mut scratch, |_| target).into_iter().next().map(|(_, r)| r)
}

fn dirt_of(refresh: Option<crate::context::Refresh>) -> Option<TargetDirt> {
  match refresh {
    None => None,
    Some(crate::context::Refresh::Rebuilt(dirt)) | Some(crate::context::Refresh::Patched(_, dirt)) => Some(dirt),
    Some(crate::context::Refresh::Unchanged) => None,
  }
}

#[test]
fn a_missing_source_mirror_or_a_capped_list_derives_full_dirt() {
  const COUNT: usize = 4;
  let records: Vec<f32> =
    (0..COUNT).flat_map(|i| list_record(8.0 + 12.0 * i as f32, 8.0, 6.0, 6.0, i as f32)).collect();
  let build = |capacity: usize| -> (Lists, Mirrors) {
    let mut mirrors = Mirrors::new();
    let extent = Some(RecordExtent { position: 0, size: 8, rotation: None, min_screen_px: None });
    mirrors.insert(BOX_BUFFER, Mirror::for_list(floats_bytes(&records), LIST_RECORD, extent));
    mirrors.insert(COLOUR_BUFFER, Mirror::for_list(vec![0u8; COUNT * LIST_COLOUR], LIST_COLOUR, None));
    let mut lists = Lists::new();
    lists.insert(LIST_KEY, test_list(COUNT, capacity, false, true));
    assert_eq!(dirt_of(point(&mut lists, &mut mirrors)), Some(TargetDirt::Full), "the first build is full");
    (lists, mirrors)
  };
  let colour = |mirrors: &mut Mirrors| mirrors.get_mut(&COLOUR_BUFFER).expect("colour mirror").write_bytes(4, &[7; 4]);
  // With every mirror: a colour write to a drawn record is rectangles.
  let (mut lists, mut mirrors) = build(COUNT);
  colour(&mut mirrors);
  assert!(matches!(dirt_of(point(&mut lists, &mut mirrors)), Some(TargetDirt::Rects(_))));
  // The box buffer destroyed while the entry still declares it: every
  // drawn record reads as zero, so the rebuild renders in full.
  let (mut lists, mut mirrors) = build(COUNT);
  mirrors.remove(&BOX_BUFFER);
  colour(&mut mirrors);
  assert_eq!(dirt_of(point(&mut lists, &mut mirrors)), Some(TargetDirt::Full), "a source without its mirror");
  // A list capped below its visible set.
  let (mut lists, mut mirrors) = build(COUNT - 1);
  colour(&mut mirrors);
  assert_eq!(dirt_of(point(&mut lists, &mut mirrors)), Some(TargetDirt::Full), "a capped list");
}

// The property run: records, rounds and the writes per round.
const PROPERTY_POPULATION: usize = 40;
const PROPERTY_MIN_POPULATION: usize = 30;
const PROPERTY_ROUNDS: usize = 300;
const PROPERTY_MAX_WRITES: usize = 6;
// Records land up to this far past the view, so some leave it.
const PROPERTY_SPREAD: f32 = 80.0;
const PROPERTY_MAX_SIZE: f32 = 14.0;
const PROPERTY_KEYS: f32 = 8.0;

// A seeded LCG: the same run every time.
struct Lcg(u64);

impl Lcg {
  fn unit(&mut self) -> f32 {
    self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (self.0 >> 40) as f32 / (1u64 << 24) as f32
  }

  fn below(&mut self, n: usize) -> usize {
    ((self.unit() * n as f32) as usize).min(n - 1)
  }
}

// What the target shows of the list: per drawn slot in draw order, its
// records (both buffers) and its box.
fn shown(list: &crate::context::ListEntry, mirrors: &Mirrors) -> Vec<(u32, Vec<u8>, CellBox)> {
  let (records, _) = mirrors[&BOX_BUFFER].record_bytes();
  let (colours, _) = mirrors[&COLOUR_BUFFER].record_bytes();
  let boxes = mirrors[&BOX_BUFFER].box_column();
  list
    .listed_slots()
    .iter()
    .map(|&s| {
      let s = s as usize;
      let mut bytes = records[s * LIST_RECORD..(s + 1) * LIST_RECORD].to_vec();
      bytes.extend_from_slice(&colours[s * LIST_COLOUR..(s + 1) * LIST_COLOUR]);
      (s as u32, bytes, CellBox::read(boxes, s))
    })
    .collect()
}

// The records drawn over the center of pixel (x, y), in draw order.
fn over(shown: &[(u32, Vec<u8>, CellBox)], x: usize, y: usize) -> Vec<(u32, Vec<u8>)> {
  let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
  shown
    .iter()
    .filter(|(_, _, b)| {
      !b.is_empty() && b.bounds[0] <= px && px <= b.bounds[3] && b.bounds[1] <= py && py <= b.bounds[4]
    })
    .map(|(s, bytes, _)| (*s, bytes.clone()))
    .collect()
}

fn inside(rects: &[PxRect], x: usize, y: usize) -> bool {
  rects.iter().any(|r| {
    (r.x as usize..r.x as usize + r.w as usize).contains(&x) && (r.y as usize..r.y as usize + r.h as usize).contains(&y)
  })
}

// Drive `refresh` through rebuild points over real mirrors and check the
// purity property at every point: outside the derived rectangles, every
// pixel draws the same records in the same order before and after.
fn damage_covers_every_change(staged: bool, seed: u64) {
  let mut rng = Lcg(seed);
  let mut stage: Vec<f32> = (0..PROPERTY_POPULATION)
    .flat_map(|_| {
      let size = 2.0 + rng.unit() * PROPERTY_MAX_SIZE;
      list_record(rng.unit() * LIST_SIDE, rng.unit() * LIST_SIDE, size, size, (rng.unit() * PROPERTY_KEYS).floor())
    })
    .collect();
  let column = |stage: &[f32]| -> Vec<f32> { stage.chunks(LIST_FLOATS).flat_map(|r| staged_box(r).floats()).collect() };
  let mut mirrors = Mirrors::new();
  let extent = (!staged).then_some(RecordExtent { position: 0, size: 8, rotation: None, min_screen_px: None });
  mirrors.insert(BOX_BUFFER, Mirror::for_list(floats_bytes(&stage), LIST_RECORD, extent));
  mirrors.insert(COLOUR_BUFFER, Mirror::for_list(vec![0u8; PROPERTY_POPULATION * LIST_COLOUR], LIST_COLOUR, None));
  if staged {
    let all = stage.len();
    mirrors.get_mut(&BOX_BUFFER).expect("box mirror").write_instances(0, all, &stage, Some(&column(&stage)));
  }
  let mut lists = Lists::new();
  lists.insert(LIST_KEY, test_list(PROPERTY_POPULATION, PROPERTY_POPULATION, true, !staged));
  point(&mut lists, &mut mirrors);
  let (mut rects_seen, mut full_seen) = (0, 0);
  for round in 0..PROPERTY_ROUNDS {
    let before = shown(&lists[&LIST_KEY], &mirrors);
    for _ in 0..1 + rng.below(PROPERTY_MAX_WRITES) {
      let slot = rng.below(PROPERTY_POPULATION);
      let r = &mut stage[slot * LIST_FLOATS..(slot + 1) * LIST_FLOATS];
      match rng.below(6) {
        0 => {
          r[0] = rng.unit() * PROPERTY_SPREAD;
          r[1] = rng.unit() * PROPERTY_SPREAD;
        }
        1 => r[2] = 0.0,
        2 => r[4] = (rng.unit() * PROPERTY_KEYS).floor(),
        3 => {
          r[2] = 2.0 + rng.unit() * PROPERTY_MAX_SIZE;
          r[3] = 2.0 + rng.unit() * PROPERTY_MAX_SIZE;
        }
        4 => {
          let colour = [rng.below(256) as u8; LIST_COLOUR];
          mirrors.get_mut(&COLOUR_BUFFER).expect("colour mirror").write_bytes(slot * LIST_COLOUR, &colour);
          continue;
        }
        _ => {
          let population = PROPERTY_MIN_POPULATION + rng.below(PROPERTY_POPULATION - PROPERTY_MIN_POPULATION + 1);
          lists.get_mut(&LIST_KEY).expect("the list").set_instances(population);
          continue;
        }
      }
      let (lo, hi) = (slot * LIST_FLOATS, (slot + 1) * LIST_FLOATS);
      let mirror = mirrors.get_mut(&BOX_BUFFER).expect("box mirror");
      if staged {
        mirror.write_instances(lo, hi, &stage, Some(&column(&stage)));
      } else {
        mirror.write_bytes(lo * 4, &floats_bytes(&stage[lo..hi]));
      }
    }
    let dirt = dirt_of(point(&mut lists, &mut mirrors));
    let after = shown(&lists[&LIST_KEY], &mirrors);
    let rects = match dirt {
      Some(TargetDirt::Full) => {
        full_seen += 1;
        continue;
      }
      Some(TargetDirt::Rects(rects)) => {
        rects_seen += 1;
        rects
      }
      None => Vec::new(),
    };
    for y in 0..LIST_SIDE as usize {
      for x in 0..LIST_SIDE as usize {
        if !inside(&rects, x, y) {
          assert_eq!(
            over(&before, x, y),
            over(&after, x, y),
            "round {round}: pixel ({x}, {y}) changed outside {rects:?}"
          );
        }
      }
    }
  }
  assert!(rects_seen > PROPERTY_ROUNDS / 2, "rectangles derived {rects_seen} times, full {full_seen} times");
}

// The seeds of the two property runs.
const EXTENT_SEED: u64 = 17;
const STAGED_SEED: u64 = 29;

#[test]
fn record_box_damage_covers_every_change() {
  damage_covers_every_change(false, EXTENT_SEED);
}

#[test]
fn staged_box_damage_covers_every_change() {
  damage_covers_every_change(true, STAGED_SEED);
}
