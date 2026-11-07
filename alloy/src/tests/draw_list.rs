use crate::gpu::{gather_list, CellBox, CellIndex, DrawList, ListChange, ListOrder, OrderKey, OrderScratch, OrderView};
use crate::spatial::Lattice;

// The target size the screen reach is measured in.
const VIEWPORT: [f32; 2] = [1000.0, 1000.0];

// A record of the tests: its sort key (float 0) and a marker (float 1).
const STRIDE: usize = 8;

// A 2d lattice of `size` square cells at `origin`.
fn lattice(size: f64, origin: [f64; 2]) -> Lattice {
  Lattice { origin: [origin[0], origin[1], 0.0], cell: [size, size, 0.0] }
}

// The y-down orthographic view of the 2d views over a `w` x `h` world
// rectangle at (x, y): clip x = 2 (p.x - x) / w - 1, clip y =
// 1 - 2 (p.y - y) / h.
fn view_rect(x: f32, y: f32, w: f32, h: f32) -> OrderView {
  let mut m = [0.0f32; 16];
  m[0] = 2.0 / w;
  m[5] = -2.0 / h;
  m[10] = -1.0;
  m[12] = -2.0 * x / w - 1.0;
  m[13] = 2.0 * y / h + 1.0;
  m[15] = 1.0;
  OrderView { clip: m }
}

// A GL perspective (near 1, far 10000, focal f) with its eye at `eye`,
// looking down -z, column-major: the projection times the translation by
// -eye.
fn perspective(f: f32, eye: [f32; 3]) -> OrderView {
  let (n, fa) = (1.0f32, 10000.0f32);
  let mut m = [0.0f32; 16];
  m[0] = f;
  m[5] = f;
  m[10] = -(fa + n) / (fa - n);
  m[11] = -1.0;
  m[14] = -2.0 * fa * n / (fa - n);
  for r in 0..4 {
    m[12 + r] -= m[r] * eye[0] + m[4 + r] * eye[1] + m[8 + r] * eye[2];
  }
  OrderView { clip: m }
}

// A `w` x `h` rectangle centered at (cx, cy) reaching `px` screen pixels
// beyond itself; a zero size draws nothing.
fn rect(cx: f32, cy: f32, w: f32, h: f32, px: f32) -> CellBox {
  if !(w > 0.0 && h > 0.0) {
    return CellBox::EMPTY;
  }
  CellBox { bounds: [cx - w * 0.5, cy - h * 0.5, 0.0, cx + w * 0.5, cy + h * 0.5, 0.0], px }
}

// The slots of `range`, as a write's changed set.
fn slots(range: std::ops::Range<usize>) -> Vec<u32> {
  range.map(|s| s as u32).collect()
}

fn column(boxes: &[CellBox]) -> Vec<f32> {
  boxes.iter().flat_map(|b| b.floats()).collect()
}

// Records keyed by `keys`, each marked with its slot.
fn records(keys: &[f32]) -> Vec<u8> {
  keys.iter().enumerate().flat_map(|(slot, k)| [*k, slot as f32]).flat_map(|f| f.to_ne_bytes()).collect()
}

fn key_order(records: &[u8], descending: bool) -> ListOrder<'_> {
  ListOrder { key: OrderKey::Field { offset: 0 }, descending, stride: STRIDE, records }
}

// The brute-force draw list: every slot of `count` whose own box reaches
// the view (any non-empty box without one), in slot order, then stably
// sorted by the order's key.
fn reference(boxes: &[f32], count: usize, view: Option<&OrderView>, order: Option<&ListOrder>) -> Vec<u32> {
  let mut slots: Vec<u32> = (0..count)
    .filter(|&s| {
      let b = CellBox::read(boxes, s);
      match view {
        Some(v) => v.reaches_box(&b, VIEWPORT),
        None => !b.is_empty(),
      }
    })
    .map(|s| s as u32)
    .collect();
  if let Some(o) = order {
    let key = |slot: u32| {
      let at = slot as usize * o.stride;
      o.records.get(at..at + 4).map_or(0.0, |b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    };
    if o.descending {
      slots.sort_by(|a, b| key(*b).total_cmp(&key(*a)));
    } else {
      slots.sort_by(|a, b| key(*a).total_cmp(&key(*b)));
    }
  }
  slots
}

fn build(
  index: &CellIndex,
  boxes: &[f32],
  view: Option<&OrderView>,
  order: Option<&ListOrder>,
  scratch: &mut OrderScratch,
) -> Vec<u32> {
  let mut list = DrawList::default();
  let n = list.build(index, boxes, view, VIEWPORT, order, scratch);
  assert_eq!(n, list.len());
  list.list().to_vec()
}

#[test]
fn a_culled_list_keeps_slot_order_across_cells() {
  // Records overlapping across the x = 100 cell boundary, in slots that
  // alternate between the two cells: a cell-ordered buffer would draw
  // 0, 2, 1, 3; the list draws slot order.
  let boxes = column(&[
    rect(95.0, 50.0, 20.0, 20.0, 0.0),
    rect(105.0, 50.0, 20.0, 20.0, 0.0),
    rect(50.0, 50.0, 20.0, 20.0, 0.0),
    rect(150.0, 50.0, 20.0, 20.0, 0.0),
    rect(1000.0, 1000.0, 20.0, 20.0, 0.0),
  ]);
  let mut index = CellIndex::new(lattice(100.0, [0.0, 0.0]));
  index.update(&boxes, 0..5);
  assert_eq!(index.cell_count(), 3);
  let mut scratch = OrderScratch::default();
  let all = build(&index, &boxes, Some(&view_rect(0.0, 0.0, 200.0, 100.0)), None, &mut scratch);
  assert_eq!(all, vec![0, 1, 2, 3]);
  let boundary = build(&index, &boxes, Some(&view_rect(90.0, 0.0, 20.0, 100.0)), None, &mut scratch);
  assert_eq!(boundary, vec![0, 1], "the two records over the boundary, in slot order");
  let unculled = build(&index, &boxes, None, None, &mut scratch);
  assert_eq!(unculled, vec![0, 1, 2, 3, 4]);
}

#[test]
fn an_edge_cell_tests_each_record_by_its_own_box_and_reach() {
  let boxes = column(&[
    rect(10.0, 10.0, 4.0, 4.0, 0.0),
    rect(90.0, 90.0, 4.0, 4.0, 0.0),
    // 10.5 units right of a 120-unit view over 1000 pixels: 87.5 pixels
    // off screen, in only with a reach past that.
    rect(130.0, 50.0, 1.0, 1.0, 100.0),
    rect(131.0, 60.0, 1.0, 1.0, 50.0),
  ]);
  let mut index = CellIndex::new(lattice(100.0, [0.0, 0.0]));
  index.update(&boxes, 0..4);
  let mut scratch = OrderScratch::default();
  let corner = build(&index, &boxes, Some(&view_rect(80.0, 80.0, 40.0, 40.0)), None, &mut scratch);
  assert_eq!(corner, vec![1], "only the record whose box the view meets in the edge cell");
  let reach = view_rect(0.0, 0.0, 120.0, 100.0);
  assert_eq!(build(&index, &boxes, Some(&reach), None, &mut scratch), vec![0, 1, 2]);
  assert_eq!(reference(&boxes, 4, Some(&reach), None), vec![0, 1, 2]);
}

#[test]
fn ties_keep_slot_order_both_ways() {
  let boxes = column(&[rect(10.0, 10.0, 4.0, 4.0, 0.0); 6]);
  // Slot 5 has no record in the key buffer: it sorts as a zero key.
  let recs = records(&[1.0, 0.0, 1.0, 0.0, 1.0]);
  let mut index = CellIndex::new(lattice(100.0, [0.0, 0.0]));
  index.update(&boxes, 0..6);
  let mut scratch = OrderScratch::default();
  let view = view_rect(0.0, 0.0, 100.0, 100.0);
  let up = build(&index, &boxes, Some(&view), Some(&key_order(&recs, false)), &mut scratch);
  assert_eq!(up, vec![1, 3, 5, 0, 2, 4]);
  let down = build(&index, &boxes, Some(&view), Some(&key_order(&recs, true)), &mut scratch);
  assert_eq!(down, vec![0, 2, 4, 1, 3, 5]);
}

// A seeded LCG: the same population every run.
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

// The side of the random world.
const WORLD: f32 = 2000.0;

// A random record box: mostly small, some large, a tenth empty, a fifth
// with a screen reach.
fn random_box(rng: &mut Lcg) -> CellBox {
  if rng.unit() < 0.1 {
    return CellBox::EMPTY;
  }
  let size = if rng.unit() < 0.05 { 300.0 } else { 1.0 + rng.unit() * 60.0 };
  let px = if rng.unit() < 0.2 { rng.unit() * 40.0 } else { 0.0 };
  rect(rng.unit() * WORLD, rng.unit() * WORLD, size, size * (0.5 + rng.unit()), px)
}

#[test]
fn the_list_equals_a_brute_force_reference_through_writes() {
  const COUNT: usize = 4000;
  // Keys from a few values, so ties are common.
  const KEY_VALUES: usize = 50;
  let mut rng = Lcg(7);
  let mut boxes: Vec<CellBox> = (0..COUNT).map(|_| random_box(&mut rng)).collect();
  let keys: Vec<f32> = (0..COUNT).map(|_| rng.below(KEY_VALUES) as f32).collect();
  let recs = records(&keys);
  let mut count = COUNT;
  let mut index = CellIndex::new(lattice(256.0, [-37.0, 11.0]));
  index.update(&column(&boxes), 0..count);
  let views = [
    view_rect(0.0, 0.0, WORLD, WORLD),
    view_rect(700.0, 500.0, 400.0, 300.0),
    view_rect(-150.0, 1800.0, 400.0, 400.0),
    view_rect(5000.0, 5000.0, 100.0, 100.0),
    view_rect(-4000.0, -4000.0, 10000.0, 10000.0),
    perspective(1.5, [1000.0, 1000.0, 1500.0]),
    perspective(1.5, [1000.0, 1000.0, 0.0]),
  ];
  let ascending = key_order(&recs, false);
  let descending = key_order(&recs, true);
  let orders = [None, Some(&ascending), Some(&descending)];
  let mut scratch = OrderScratch::default();
  for round in 0..12 {
    if round > 0 {
      // Rewrite a random range (records move across cells, empty, come
      // back), and every fourth round shrink or regrow the population.
      let start = rng.below(count);
      let end = (start + 1 + rng.below(600)).min(count);
      for b in &mut boxes[start..end] {
        *b = random_box(&mut rng);
      }
      index.update(&column(&boxes[..count]), start..end);
      if round % 4 == 0 {
        let next = if count == COUNT { COUNT - 500 } else { COUNT };
        index.update(&column(&boxes[..next]), next.min(count)..next.max(count));
        count = next;
      }
    }
    let col = column(&boxes[..count]);
    for view in views.iter().map(Some).chain([None]) {
      for order in orders {
        let got = build(&index, &col, view, order, &mut scratch);
        let want = reference(&col, count, view, order);
        assert_eq!(got, want, "round {round}, view {view:?}, descending {:?}", order.map(|o| o.descending));
      }
    }
    // The windows cull: some records in, most out.
    let window = build(&index, &col, Some(&views[1]), None, &mut scratch).len();
    assert!(window > 0 && window < count / 4, "round {round}: {window} of {count} in the window");
    assert!(build(&index, &col, Some(&views[3]), None, &mut scratch).is_empty());
  }
  // The index kept through the writes equals one built from scratch.
  let col = column(&boxes[..count]);
  let mut fresh = CellIndex::new(*index.lattice());
  fresh.update(&col, 0..count);
  assert_eq!(index.cell_count(), fresh.cell_count());
  assert_eq!(index.span(), fresh.span());
  for slot in 0..COUNT {
    assert_eq!(index.cell_of(slot), fresh.cell_of(slot), "slot {slot}");
  }
}

// Small moves, most of which keep a record where it was drawn or not.
const JITTER: f32 = 6.0;

#[test]
fn a_kept_or_patched_list_equals_a_fresh_build_through_writes() {
  const COUNT: usize = 3000;
  const KEY_VALUES: usize = 40;
  let mut rng = Lcg(11);
  let mut boxes: Vec<CellBox> = (0..COUNT).map(|_| random_box(&mut rng)).collect();
  let mut keys: Vec<f32> = (0..COUNT).map(|_| rng.below(KEY_VALUES) as f32).collect();
  let mut index = CellIndex::new(lattice(256.0, [0.0, 0.0]));
  index.update(&column(&boxes), 0..COUNT);
  let views = [view_rect(700.0, 500.0, 400.0, 300.0), view_rect(0.0, 0.0, WORLD, WORLD)];
  let mut scratch = OrderScratch::default();
  // One kept list per view and order (none, ascending, descending).
  fn order_of(o: usize, recs: &[u8]) -> Option<ListOrder<'_>> {
    (o > 0).then(|| key_order(recs, o == 2))
  }
  let mut lists: Vec<DrawList> = (0..views.len() * 3).map(|_| DrawList::default()).collect();
  {
    let recs = records(&keys);
    let col = column(&boxes);
    for (i, list) in lists.iter_mut().enumerate() {
      list.build(&index, &col, Some(&views[i / 3]), VIEWPORT, order_of(i % 3, &recs).as_ref(), &mut scratch);
    }
  }
  let (mut patches, mut rebuilds) = (0, 0);
  for round in 0..200 {
    // Jitter a short range; now and then a key or a jump across the world.
    let start = rng.below(COUNT);
    let end = (start + 1 + rng.below(8)).min(COUNT);
    for slot in start..end {
      if rng.unit() < 0.05 {
        boxes[slot] = random_box(&mut rng);
      } else if !boxes[slot].is_empty() {
        let (dx, dy) = ((rng.unit() - 0.5) * JITTER, (rng.unit() - 0.5) * JITTER);
        let b = &mut boxes[slot].bounds;
        b[0] += dx;
        b[3] += dx;
        b[1] += dy;
        b[4] += dy;
      }
      if rng.unit() < 0.1 {
        keys[slot] = rng.below(KEY_VALUES) as f32;
      }
    }
    let recs = records(&keys);
    let col = column(&boxes);
    index.update(&col, start..end);
    for (i, list) in lists.iter_mut().enumerate() {
      let (view, order) = (&views[i / 3], order_of(i % 3, &recs));
      let change = list.change_culled(&slots(start..end), &index, &col, Some(view), VIEWPORT, order.as_ref());
      let fresh = build(&index, &col, Some(view), order.as_ref(), &mut scratch);
      match change {
        ListChange::None => assert_eq!(list.list(), &fresh[..], "round {round}, list {i}: kept"),
        ListChange::Patch(span) => {
          patches += 1;
          assert_eq!(list.list(), &fresh[..], "round {round}, list {i}: patched");
          // Every written slot that is drawn sits inside the patch's span.
          for (p, &slot) in list.list().iter().enumerate() {
            if (start..end).contains(&(slot as usize)) {
              assert!(span.contains(&p), "round {round}, list {i}: slot {slot} at {p} outside {span:?}");
            }
          }
        }
        ListChange::Rebuild => {
          rebuilds += 1;
          list.build(&index, &col, Some(view), VIEWPORT, order.as_ref(), &mut scratch);
          assert_eq!(list.list(), &fresh[..], "round {round}, list {i}: rebuilt");
        }
      }
    }
  }
  assert!(patches > 0 && rebuilds > 0, "{patches} patches, {rebuilds} rebuilds");
}

#[test]
fn an_emptied_population_leaves_no_cell() {
  let mut boxes = vec![rect(10.0, 10.0, 4.0, 4.0, 0.0), rect(310.0, 10.0, 4.0, 4.0, 0.0)];
  let mut index = CellIndex::new(lattice(100.0, [0.0, 0.0]));
  index.update(&column(&boxes), 0..2);
  assert_eq!((index.cell_count(), index.span()), (2, 2));
  boxes[1] = CellBox::EMPTY;
  index.update(&column(&boxes), 1..2);
  assert_eq!((index.cell_count(), index.span()), (1, 1));
  index.update(&[], 0..1);
  assert_eq!((index.cell_count(), index.span()), (0, 0));
  assert_eq!(index.cell_of(0), None);
}

#[test]
fn change_patches_a_write_that_keeps_the_drawn_set_and_the_keys() {
  let mut boxes =
    vec![rect(10.0, 10.0, 4.0, 4.0, 0.0), rect(500.0, 500.0, 4.0, 4.0, 0.0), rect(20.0, 20.0, 4.0, 4.0, 0.0)];
  let mut index = CellIndex::new(lattice(100.0, [0.0, 0.0]));
  index.update(&column(&boxes), 0..3);
  let view = view_rect(0.0, 0.0, 100.0, 100.0);
  let mut list = DrawList::default();
  let mut scratch = OrderScratch::default();
  list.build(&index, &column(&boxes), Some(&view), VIEWPORT, None, &mut scratch);
  assert_eq!(list.list(), &[0, 2]);
  let change = |list: &DrawList, index: &CellIndex, boxes: &[CellBox], dirty: std::ops::Range<usize>| {
    list.change_culled(&slots(dirty), index, &column(boxes), Some(&view), VIEWPORT, None)
  };
  // Off screen and staying there: nothing to do.
  boxes[1] = rect(600.0, 500.0, 4.0, 4.0, 0.0);
  index.update(&column(&boxes), 1..2);
  assert_eq!(change(&list, &index, &boxes, 1..2), ListChange::None);
  // A drawn record moving within the view: its list position, re-gathered.
  boxes[2] = rect(30.0, 25.0, 4.0, 4.0, 0.0);
  index.update(&column(&boxes), 2..3);
  assert_eq!(change(&list, &index, &boxes, 2..3), ListChange::Patch(1..2));
  // Two drawn slots: one span over both.
  assert_eq!(change(&list, &index, &boxes, 0..3), ListChange::Patch(0..2));
  // Coming into view, or leaving it: the drawn set changes.
  boxes[1] = rect(50.0, 50.0, 4.0, 4.0, 0.0);
  assert_eq!(change(&list, &index, &boxes, 1..2), ListChange::Rebuild);
  boxes[1] = rect(600.0, 500.0, 4.0, 4.0, 0.0);
  boxes[0] = rect(900.0, 900.0, 4.0, 4.0, 0.0);
  assert_eq!(change(&list, &index, &boxes, 0..1), ListChange::Rebuild);
  // Emptied: it leaves the drawn set.
  boxes[0] = CellBox::EMPTY;
  assert_eq!(change(&list, &index, &boxes, 0..1), ListChange::Rebuild);
}

#[test]
fn change_rebuilds_when_a_drawn_key_moves() {
  let keys = [3.0, 1.0, 2.0];
  let recs = records(&keys);
  let order = key_order(&recs, false);
  let mut list = DrawList::default();
  let mut scratch = OrderScratch::default();
  list.build_all(3, Some(&order), &mut scratch);
  assert_eq!(list.list(), &[1, 2, 0]);
  // The key as it was (another field written): patched at its position.
  assert_eq!(list.change_all(&[0], Some(&order)), ListChange::Patch(2..3));
  // A new key, even one keeping the order: built again.
  let moved = records(&[3.5, 1.0, 2.0]);
  assert_eq!(list.change_all(&[0], Some(&key_order(&moved, false))), ListChange::Rebuild);
  // Unordered, every write patches.
  list.build_all(3, None, &mut scratch);
  assert_eq!(list.change_all(&[1, 2], None), ListChange::Patch(1..3));
}

#[test]
fn a_patch_spans_its_changed_positions() {
  const COUNT: usize = 1000;
  let mut list = DrawList::default();
  let mut scratch = OrderScratch::default();
  list.build_all(COUNT, None, &mut scratch);
  // One span, from the first changed position through the last.
  assert_eq!(list.change_all(&[14], None), ListChange::Patch(14..15));
  assert_eq!(list.change_all(&[10, 500, 999], None), ListChange::Patch(10..1000));
  // Under a key, the span is over list positions, not slots.
  let recs = records(&[3.0, 1.0, 2.0]);
  let order = key_order(&recs, false);
  list.build_all(3, Some(&order), &mut scratch);
  assert_eq!(list.list(), &[1, 2, 0]);
  assert_eq!(list.change_all(&[0, 1], Some(&order)), ListChange::Patch(0..3));
  assert_eq!(list.change_all(&[1, 2], Some(&order)), ListChange::Patch(0..2));
}

#[test]
fn a_margin_grows_every_box_the_index_and_the_list_test() {
  // A box 30 units right of the view's edge: drawn only with a margin
  // that reaches it.
  let boxes = vec![rect(132.0, 50.0, 4.0, 4.0, 0.0), CellBox::EMPTY];
  let view = view_rect(0.0, 0.0, 100.0, 100.0);
  let mut scratch = OrderScratch::default();
  for (margin, drawn) in [(0.0, 0), (20.0, 0), (40.0, 1)] {
    let mut index = CellIndex::new(lattice(100.0, [0.0, 0.0])).with_margin(margin);
    index.update(&column(&boxes), 0..2);
    assert_eq!(build(&index, &column(&boxes), Some(&view), None, &mut scratch).len(), drawn, "margin {margin}");
  }
  // An empty box stays empty, whatever the margin.
  assert!(CellBox::EMPTY.grown(40.0).is_empty());
}

#[test]
fn gather_keeps_every_buffer_aligned_on_the_list() {
  let a: Vec<u8> = (0..4u32).flat_map(|i| [i * 10, i * 10 + 1]).flat_map(|v| v.to_ne_bytes()).collect();
  let b: Vec<u8> = (0..3u32).flat_map(|i| (i * 100).to_ne_bytes()).collect();
  let list = [3u32, 0, 2];
  let mut da = vec![0xffu8; 3 * 8];
  let mut db = vec![0xffu8; 3 * 4];
  gather_list(&list, 8, &a, &mut da);
  gather_list(&list, 4, &b, &mut db);
  let words = |v: &[u8]| v.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect::<Vec<_>>();
  assert_eq!(words(&da), vec![30, 31, 0, 1, 20, 21]);
  assert_eq!(words(&db), vec![0, 0, 200], "slot 3 is past the second buffer: a zero record");
}
