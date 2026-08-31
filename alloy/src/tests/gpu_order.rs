use crate::gpu::{gather_ordered, gather_permuted, order_permutation, InstanceOrder, OrderKey, OrderScratch};

// Pack f32 records into the byte shape the lease block holds.
fn bytes(floats: &[f32]) -> Vec<u8> {
  floats.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

// The f32 at float offset `at` of record `i` in a gathered byte block.
fn record_float(block: &[u8], stride: usize, i: usize, at: usize) -> f32 {
  let mut b = [0u8; 4];
  b.copy_from_slice(&block[i * stride + at * 4..i * stride + at * 4 + 4]);
  f32::from_ne_bytes(b)
}

fn field(offset_floats: usize) -> InstanceOrder {
  InstanceOrder { key: OrderKey::Field { offset: offset_floats * 4 }, descending: false, key_slot: 0 }
}

#[test]
fn parse_validates_the_key_shape() {
  let both = InstanceOrder::parse(Some(0.0), Some(0.0), Some([0.0, 0.0, 1.0]), false, None);
  assert!(both.expect_err("both keys").contains("not both"));
  let neither = InstanceOrder::parse(None, None, None, false, None);
  assert!(neither.expect_err("no key").contains("needs a key"));
  let dir_with_field = InstanceOrder::parse(Some(1.0), None, Some([0.0, 0.0, 1.0]), false, None);
  assert!(dir_with_field.expect_err("direction with field").contains("field key has none"));
  let no_dir = InstanceOrder::parse(None, Some(0.0), None, false, None);
  assert!(no_dir.expect_err("position without direction").contains("needs a direction"));
  let fractional = InstanceOrder::parse(Some(1.5), None, None, false, None);
  assert!(fractional.expect_err("fractional offset").contains("non-negative integer"));
  let negative = InstanceOrder::parse(Some(-1.0), None, None, false, None);
  assert!(negative.expect_err("negative offset").contains("non-negative integer"));
  let zero_dir = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 0.0]), false, None);
  assert!(zero_dir.expect_err("zero direction").contains("zero vector"));
  let nan_dir = InstanceOrder::parse(None, Some(0.0), Some([f32::NAN, 0.0, 1.0]), false, None);
  assert!(nan_dir.expect_err("nan direction").contains("finite"));
  // Float offsets store as bytes.
  let ok = InstanceOrder::parse(Some(2.0), None, None, true, None).expect("field key parses");
  assert_eq!(ok.key, OrderKey::Field { offset: 8 });
  assert!(ok.descending);
  assert_eq!(ok.key_slot, 0, "slot defaults to 0");
  // The key slot: an instance-slot index, bounds-checked at parse.
  let slotted = InstanceOrder::parse(Some(0.0), None, None, false, Some(1.0)).expect("slot 1 parses");
  assert_eq!(slotted.key_slot, 1);
  let big = InstanceOrder::parse(Some(0.0), None, None, false, Some(4.0));
  assert!(big.expect_err("slot past the last").contains("integer 0.."));
  let fractional_slot = InstanceOrder::parse(Some(0.0), None, None, false, Some(0.5));
  assert!(fractional_slot.expect_err("fractional slot").contains("integer 0.."));
  let negative_slot = InstanceOrder::parse(Some(0.0), None, None, false, Some(-1.0));
  assert!(negative_slot.expect_err("negative slot").contains("integer 0.."));
}

#[test]
fn check_stride_bounds_the_key_bytes() {
  // A 16-byte record: float offsets 0..3 hold an f32, 3 is the last that fits.
  field(3).check_stride(16).expect("last float fits");
  assert!(field(4).check_stride(16).expect_err("one past").contains("does not fit"));
  let projected =
    InstanceOrder { key: OrderKey::Projected { offset: 4, direction: [0.0, 0.0, 1.0] }, descending: false, key_slot: 0 };
  projected.check_stride(16).expect("vec3 at float 1 fits a 16-byte record");
  assert!(projected.check_stride(12).expect_err("vec3 past the record").contains("does not fit"));
}

#[test]
fn set_direction_is_projected_only() {
  let mut f = field(0);
  assert!(f.set_direction([0.0, 1.0, 0.0]).expect_err("field key").contains("field key"));
  let mut p = InstanceOrder { key: OrderKey::Projected { offset: 0, direction: [1.0, 0.0, 0.0] }, descending: false, key_slot: 0 };
  assert!(p.set_direction([0.0, 0.0, 0.0]).expect_err("zero direction").contains("zero vector"));
  p.set_direction([0.0, 2.0, 0.0]).expect("replace");
  assert_eq!(p.key, OrderKey::Projected { offset: 0, direction: [0.0, 2.0, 0.0] });
}

#[test]
fn gather_sorts_ascending_and_keeps_ties_stable() {
  // Two floats per record: [key, slot marker]. Keys cover negatives, a tie,
  // and NaN (which sorts last ascending).
  let src = bytes(&[3.0, 0.0, -1.0, 1.0, 3.0, 2.0, f32::NAN, 3.0, 0.0, 4.0]);
  let stride = 8;
  let mut dst = vec![0u8; src.len()];
  let mut scratch = OrderScratch::default();
  gather_ordered(&field(0), stride, &src, &mut dst, &mut scratch);
  let order: Vec<f32> = (0..5).map(|i| record_float(&dst, stride, i, 1)).collect();
  // -1 first, then 0, then the two 3.0 records in slot order (stable), NaN last.
  assert_eq!(order, vec![1.0, 4.0, 0.0, 2.0, 3.0]);
}

#[test]
fn gather_descending_reverses() {
  let src = bytes(&[1.0, 0.0, 3.0, 1.0, 2.0, 2.0]);
  let stride = 8;
  let mut dst = vec![0u8; src.len()];
  let mut scratch = OrderScratch::default();
  let order = InstanceOrder { key: OrderKey::Field { offset: 0 }, descending: true, key_slot: 0 };
  gather_ordered(&order, stride, &src, &mut dst, &mut scratch);
  let got: Vec<f32> = (0..3).map(|i| record_float(&dst, stride, i, 1)).collect();
  assert_eq!(got, vec![1.0, 2.0, 0.0]);
}

#[test]
fn projected_key_follows_the_direction() {
  // Records: [x, y, z, slot marker]; three positions along +z.
  let src = bytes(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 3.0, 1.0, 0.0, 0.0, 2.0, 2.0]);
  let stride = 16;
  let mut dst = vec![0u8; src.len()];
  let mut scratch = OrderScratch::default();
  let mut order =
    InstanceOrder { key: OrderKey::Projected { offset: 0, direction: [0.0, 0.0, 1.0] }, descending: false, key_slot: 0 };
  gather_ordered(&order, stride, &src, &mut dst, &mut scratch);
  let got: Vec<f32> = (0..3).map(|i| record_float(&dst, stride, i, 3)).collect();
  assert_eq!(got, vec![0.0, 2.0, 1.0], "depth ascending along +z");
  // The per-camera-move update: flip the direction, the order flips.
  order.set_direction([0.0, 0.0, -1.0]).expect("replace direction");
  gather_ordered(&order, stride, &src, &mut dst, &mut scratch);
  let got: Vec<f32> = (0..3).map(|i| record_float(&dst, stride, i, 3)).collect();
  assert_eq!(got, vec![1.0, 2.0, 0.0], "depth ascending along -z");
}

#[test]
fn gather_round_trips_a_large_shuffled_population() {
  // Sprite-shaped records (13 floats), 30k of them, keys from a simple LCG:
  // the radix path must produce a sorted permutation of exactly the input.
  const COUNT: usize = 30_000;
  let stride_floats = 13;
  let mut floats = vec![0.0f32; COUNT * stride_floats];
  let mut state: u32 = 12345;
  for slot in 0..COUNT {
    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    let key = (state >> 8) as f32 / 1000.0 - 8000.0;
    floats[slot * stride_floats] = 0.0; // cx
    floats[slot * stride_floats + 1] = key; // cy - the orderBy "y" shape
    floats[slot * stride_floats + 2] = slot as f32; // marker
  }
  let src = bytes(&floats);
  let stride = stride_floats * 4;
  let mut dst = vec![0u8; src.len()];
  let mut scratch = OrderScratch::default();
  gather_ordered(&field(1), stride, &src, &mut dst, &mut scratch);
  let mut seen = vec![false; COUNT];
  let mut last = f32::NEG_INFINITY;
  for i in 0..COUNT {
    let key = record_float(&dst, stride, i, 1);
    assert!(key >= last, "keys must be non-decreasing at record {i}");
    last = key;
    let marker = record_float(&dst, stride, i, 2) as usize;
    assert!(!seen[marker], "record {marker} gathered twice");
    seen[marker] = true;
  }
  // Scratch reuse: a second gather over the same population is identical.
  let mut dst2 = vec![0u8; src.len()];
  gather_ordered(&field(1), stride, &src, &mut dst2, &mut scratch);
  assert_eq!(dst, dst2, "reused scratch must not change the result");
}

#[test]
fn permutation_plus_permuted_gather_equals_gather_ordered() {
  // The multi-buffer split: computing the permutation from a key buffer and
  // applying it must reproduce the one-shot gather exactly.
  let src = bytes(&[3.0, 0.0, -1.0, 1.0, 3.0, 2.0, 0.0, 3.0]);
  let stride = 8;
  let mut scratch = OrderScratch::default();
  let mut one_shot = vec![0u8; src.len()];
  gather_ordered(&field(0), stride, &src, &mut one_shot, &mut scratch);
  order_permutation(&field(0), stride, &src, &mut scratch);
  let mut split = vec![0u8; src.len()];
  gather_permuted(scratch.perm(), stride, &src, &mut split);
  assert_eq!(one_shot, split);
}

#[test]
fn permuted_gather_follows_a_sibling_stride() {
  // The key buffer's permutation applied to a sibling buffer with a
  // DIFFERENT record stride - the pose/style split. Keys [2, 0, 1] order
  // slots 1, 2, 0; the one-float sibling records follow.
  let keys = bytes(&[2.0, 9.0, 0.0, 9.0, 1.0, 9.0]);
  let mut scratch = OrderScratch::default();
  order_permutation(&field(0), 8, &keys, &mut scratch);
  let sibling = bytes(&[10.0, 11.0, 12.0]);
  let mut dst = vec![0u8; sibling.len()];
  gather_permuted(scratch.perm(), 4, &sibling, &mut dst);
  let got: Vec<f32> = (0..3).map(|i| record_float(&dst, 4, i, 0)).collect();
  assert_eq!(got, vec![11.0, 12.0, 10.0]);
}

#[test]
fn permuted_gather_reconciles_count_mismatches() {
  // perm [2, 0, 1] over a 2-record publish: slot 2 is out of range and
  // skipped, the rest keep permutation order.
  let src = bytes(&[10.0, 11.0]);
  let mut dst = vec![0u8; src.len()];
  gather_permuted(&[2, 0, 1], 4, &src, &mut dst);
  let got: Vec<f32> = (0..2).map(|i| record_float(&dst, 4, i, 0)).collect();
  assert_eq!(got, vec![10.0, 11.0], "out-of-range perm entries are skipped");
  // perm [1, 0] over a 4-record publish: the unpermuted tail appends in
  // slot order.
  let src = bytes(&[10.0, 11.0, 12.0, 13.0]);
  let mut dst = vec![0u8; src.len()];
  gather_permuted(&[1, 0], 4, &src, &mut dst);
  let got: Vec<f32> = (0..4).map(|i| record_float(&dst, 4, i, 0)).collect();
  assert_eq!(got, vec![11.0, 10.0, 12.0, 13.0], "records past the permutation append in slot order");
  // An empty permutation is the identity.
  let mut dst = vec![0u8; src.len()];
  gather_permuted(&[], 4, &src, &mut dst);
  assert_eq!(src, dst);
}

#[test]
fn gather_handles_the_trivial_populations() {
  let mut scratch = OrderScratch::default();
  // Zero records.
  let mut empty: Vec<u8> = Vec::new();
  gather_ordered(&field(0), 8, &[], &mut empty, &mut scratch);
  // One record copies through.
  let src = bytes(&[5.0, 7.0]);
  let mut dst = vec![0u8; src.len()];
  gather_ordered(&field(0), 8, &src, &mut dst, &mut scratch);
  assert_eq!(src, dst);
}
