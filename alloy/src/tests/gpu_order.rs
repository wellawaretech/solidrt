use crate::gpu::{
  gather_ordered, gather_permuted, materialize_indices, materialize_indices_mapped, order_permutation,
  order_permutation_packed, InstanceOrder, KeyColumn, OrderKey, OrderScratch, OrderWorker, PackedFormat, PackedKey,
  PackedRecords, SortJob, INDEX_NONE,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
  InstanceOrder {
    key: OrderKey::Field { offset: offset_floats * 4 },
    descending: false,
    key_buffer: None,
    retain: false,
    indices: None,
  }
}

#[test]
fn parse_validates_the_key_shape() {
  let both = InstanceOrder::parse(Some(0.0), Some(0.0), Some([0.0, 0.0, 1.0]), false, None, false, None);
  assert!(both.expect_err("both keys").contains("not both"));
  let neither = InstanceOrder::parse(None, None, None, false, None, false, None);
  assert!(neither.expect_err("no key").contains("needs a key"));
  let dir_with_field = InstanceOrder::parse(Some(1.0), None, Some([0.0, 0.0, 1.0]), false, None, false, None);
  assert!(dir_with_field.expect_err("direction with field").contains("field key has none"));
  let no_dir = InstanceOrder::parse(None, Some(0.0), None, false, None, false, None);
  assert!(no_dir.expect_err("position without direction").contains("needs a direction"));
  let fractional = InstanceOrder::parse(Some(1.5), None, None, false, None, false, None);
  assert!(fractional.expect_err("fractional offset").contains("non-negative integer"));
  let negative = InstanceOrder::parse(Some(-1.0), None, None, false, None, false, None);
  assert!(negative.expect_err("negative offset").contains("non-negative integer"));
  let zero_dir = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 0.0]), false, None, false, None);
  assert!(zero_dir.expect_err("zero direction").contains("zero vector"));
  let nan_dir = InstanceOrder::parse(None, Some(0.0), Some([f32::NAN, 0.0, 1.0]), false, None, false, None);
  assert!(nan_dir.expect_err("nan direction").contains("finite"));
  // Float offsets store as bytes.
  let ok = InstanceOrder::parse(Some(2.0), None, None, true, None, false, None).expect("field key parses");
  assert_eq!(ok.key, OrderKey::Field { offset: 8 });
  assert!(ok.descending);
  assert_eq!(ok.key_buffer, None, "the key buffer defaults to the first instance-step one");
  // The key buffer: a pipeline buffer index, bounds-checked at parse.
  let slotted = InstanceOrder::parse(Some(0.0), None, None, false, Some(1.0), false, None).expect("buffer 1 parses");
  assert_eq!(slotted.key_buffer, Some(1));
  let big = InstanceOrder::parse(Some(0.0), None, None, false, Some(8.0), false, None);
  assert!(big.expect_err("buffer past the last").contains("integer 0.."));
  let fractional_slot = InstanceOrder::parse(Some(0.0), None, None, false, Some(0.5), false, None);
  assert!(fractional_slot.expect_err("fractional slot").contains("integer 0.."));
  let negative_slot = InstanceOrder::parse(Some(0.0), None, None, false, Some(-1.0), false, None);
  assert!(negative_slot.expect_err("negative slot").contains("integer 0.."));
  // The retained-copy strategy: position keys only (a field key re-orders
  // when its records republish, so retaining buys nothing).
  let retained = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), true, None, true, None)
    .expect("retain with a position key parses");
  assert!(retained.retain);
  let retained_field = InstanceOrder::parse(Some(0.0), None, None, false, None, true, None);
  assert!(retained_field.expect_err("retain with a field key").contains("retain applies to a position key"));
}

#[test]
fn check_stride_bounds_the_key_bytes() {
  // A 16-byte record: float offsets 0..3 hold an f32, 3 is the last that fits.
  field(3).check_stride(16).expect("last float fits");
  assert!(field(4).check_stride(16).expect_err("one past").contains("does not fit"));
  let projected = InstanceOrder {
    key: OrderKey::Projected { offset: 4, direction: [0.0, 0.0, 1.0] },
    descending: false,
    key_buffer: None,
    retain: false,
    indices: None,
  };
  projected.check_stride(16).expect("vec3 at float 1 fits a 16-byte record");
  assert!(projected.check_stride(12).expect_err("vec3 past the record").contains("does not fit"));
}

#[test]
fn set_direction_is_projected_only() {
  let mut f = field(0);
  assert!(f.set_direction([0.0, 1.0, 0.0]).expect_err("field key").contains("field key"));
  let mut p = InstanceOrder {
    key: OrderKey::Projected { offset: 0, direction: [1.0, 0.0, 0.0] },
    descending: false,
    key_buffer: None,
    retain: false,
    indices: None,
  };
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
  let order = InstanceOrder {
    key: OrderKey::Field { offset: 0 },
    descending: true,
    key_buffer: None,
    retain: false,
    indices: None,
  };
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
  let mut order = InstanceOrder {
    key: OrderKey::Projected { offset: 0, direction: [0.0, 0.0, 1.0] },
    descending: false,
    key_buffer: None,
    retain: false,
    indices: None,
  };
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

// The index materialization: `indices.stride` is the handed-off record's
// byte stride (a positive multiple of 4 the key must fit), stored as-is;
// the sorted permutation writes as u32 ids with the sentinel filling the
// rest of the buffer, so no stale id survives a shrink.
#[test]
fn parse_and_materialize_indices() {
  let indexed = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(32.0))
    .expect("an indexed position order parses");
  assert_eq!(indexed.indices, Some(32));
  assert_eq!(indexed.mirror_stride(64), 12, "the mirror holds the key column, not the id stream or the record");
  assert_eq!(field(0).mirror_stride(52), 52, "without indices the instance record is the record");
  let odd = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), false, None, false, Some(30.0));
  assert!(odd.expect_err("a stride off the float grid").contains("multiple of 4"));
  let zero = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), false, None, false, Some(0.0));
  assert!(zero.expect_err("a zero stride").contains("positive"));
  let tight = InstanceOrder::parse(None, Some(2.0), Some([0.0, 0.0, 1.0]), false, None, false, Some(16.0));
  assert!(tight.expect_err("a vec3 at float 2 of a 16-byte record").contains("does not fit"));

  // Three sorted slots into a buffer of six id slots: ids, then sentinels.
  let perm = [2u32, 0, 1];
  let mut dst = vec![0xAAu8; 6 * 4];
  materialize_indices(&perm, &mut dst);
  let ids: Vec<u32> = dst.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
  assert_eq!(ids, vec![2, 0, 1, INDEX_NONE, INDEX_NONE, INDEX_NONE]);
  // A trailing partial u32 is left alone; an empty permutation is all sentinel.
  let mut odd = vec![0x11u8; 9];
  materialize_indices(&[], &mut odd);
  assert_eq!(&odd[..8], &[0xFF; 8]);
  assert_eq!(odd[8], 0x11);
}

// The hand-off's key column: parsed from the API's byte stride and float
// offset, it names what the indexed mirror keeps of each record, and the
// entry's order must describe the same column when it attaches.
#[test]
fn key_column_parses_and_matches_the_order() {
  let column = KeyColumn::parse(32.0, None, Some(0.0), None).expect("a position key at float 0 of 32-byte records");
  assert_eq!(column, KeyColumn { stride: 32, offset: 0, len: 12, packed: None });
  let field_column = KeyColumn::parse(16.0, Some(3.0), None, None).expect("a field key at float 3 of 16-byte records");
  assert_eq!(field_column, KeyColumn { stride: 16, offset: 12, len: 4, packed: None });
  let indexed = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(32.0))
    .expect("an indexed position order parses");
  assert_eq!(indexed.key_column(), Some(column), "the order expects the column the hand-off declared");
  assert_eq!(field(0).key_column(), None, "a gathered order keeps whole records");
  let shifted =
    InstanceOrder::parse(None, Some(1.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(32.0)).expect("parses");
  assert_ne!(shifted.key_column(), Some(column), "a different offset is a different column");

  let both = KeyColumn::parse(32.0, Some(0.0), Some(0.0), None);
  assert!(both.expect_err("both keys").contains("not both"));
  let neither = KeyColumn::parse(32.0, None, None, None);
  assert!(neither.expect_err("no key").contains("needs field or position"));
  let odd = KeyColumn::parse(30.0, None, Some(0.0), None);
  assert!(odd.expect_err("a stride off the float grid").contains("multiple of 4"));
  let tight = KeyColumn::parse(16.0, None, Some(2.0), None);
  assert!(tight.expect_err("a vec3 at float 2 of a 16-byte record").contains("does not fit"));
  let fraction = KeyColumn::parse(16.0, None, Some(0.5), None);
  assert!(fraction.expect_err("a fractional offset").contains("float offset"));
}

// Extraction keeps the key bytes of every record, in slot order, and the
// sort reads the column with the key rebased to offset 0 - the same
// permutation the whole records would give.
#[test]
fn key_column_extracts_what_the_sort_reads() {
  // Three 16-byte records: [x y z w], the position at float 1.
  let records = bytes(&[9.0, 1.0, 2.0, 3.0, 9.0, 4.0, 5.0, 6.0, 9.0, 7.0, 8.0, 9.0]);
  let column = KeyColumn::parse(16.0, None, Some(1.0), None).expect("parses");
  assert!(column.check_records(records.len()).is_ok());
  assert!(column.check_records(records.len() - 1).expect_err("a torn record").contains("whole number"));
  let mut keys = vec![0xAAu8; 3];
  column.extract(&records, &mut keys);
  assert_eq!(keys, bytes(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]), "the column replaces the vector");

  // Depth along +z, descending: slot 2 (z = 9) first.
  let order =
    InstanceOrder::parse(None, Some(1.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(16.0)).expect("parses");
  let mut scratch = OrderScratch::default();
  order_permutation(&order.mirror_order(), order.mirror_stride(64), &keys, &mut scratch);
  assert_eq!(scratch.perm(), &[2, 1, 0]);
  let mut whole = OrderScratch::default();
  order_permutation(&order, 16, &records, &mut whole);
  assert_eq!(scratch.perm(), whole.perm(), "the column sorts as the records do");
  assert_eq!(field(2).mirror_order(), field(2), "a gathered order reads its records as declared");
}

// The sort thread: a job's key column sorts to the same permutation the
// in-place sort gives, the ids land in the job's block with the sentinel
// past them, the `done` hook ran, and dropping the worker joins the
// thread.
#[test]
fn sort_worker_round_trips_a_job() {
  let keys = Arc::new(bytes(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]));
  let order = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(12.0))
    .expect("parses")
    .mirror_order();
  let worker = OrderWorker::spawn().expect("a sort thread");
  let done = Arc::new(AtomicBool::new(false));
  let flag = done.clone();
  let job = SortJob {
    entry: (7, 1),
    buffer: 3,
    generation: 1,
    order,
    stride: 12,
    packed: None,
    keys: keys.clone(),
    len: keys.len(),
    perm: Vec::new(),
    block: vec![0u8; 5 * 4],
    done: Some(Box::new(move || flag.store(true, Ordering::Relaxed))),
  };
  assert!(worker.submit(job));
  let mut results = Vec::new();
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
  while results.is_empty() && std::time::Instant::now() < deadline {
    results = worker.drain();
    std::thread::sleep(std::time::Duration::from_millis(1));
  }
  let result = results.pop().expect("the job came back");
  assert_eq!((result.entry, result.buffer, result.generation), ((7, 1), 3, 1));
  assert_eq!(result.perm, vec![2, 1, 0], "descending depth along +z");
  let ids: Vec<u32> = result.block.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
  assert_eq!(ids, vec![2, 1, 0, INDEX_NONE, INDEX_NONE]);
  assert!(done.load(Ordering::Relaxed), "the done hook ran");
  assert!(result.work_ms >= 0.0);
  drop(worker);
}

// A packed position key: the hand-off's column names the packing and the
// chunk size, an entry's float position key over the same stride and
// offset reads it, a field key cannot be packed.
#[test]
fn packed_key_column_parses_and_matches_the_order() {
  let packed = PackedKey { format: PackedFormat::parse("unorm16x3").expect("the format"), chunk: 256 };
  let column = KeyColumn::parse(16.0, None, Some(0.0), Some(packed)).expect("a packed position at float 0");
  assert_eq!(column, KeyColumn { stride: 16, offset: 0, len: 8, packed: Some(packed) });
  assert_eq!(column.mirror_len(), 12, "the mirror holds the id and the two words");
  let indexed = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(16.0))
    .expect("an indexed position order over 16-byte records");
  assert!(column.accepts(indexed.key_column().expect("indexed")), "a position key reads the packed column");
  let shifted =
    InstanceOrder::parse(None, Some(1.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(16.0)).expect("parses");
  assert!(!column.accepts(shifted.key_column().expect("indexed")), "a different offset is a different column");
  let float = KeyColumn::parse(16.0, None, Some(0.0), None).expect("a float position");
  assert!(!float.accepts(column), "a float hand-off is not read as packed");

  let field = KeyColumn::parse(16.0, Some(0.0), None, Some(packed));
  assert!(field.expect_err("a packed field").contains("position"));
  let empty = KeyColumn::parse(16.0, None, Some(0.0), Some(PackedKey { chunk: 0, ..packed }));
  assert!(empty.expect_err("a zero chunk").contains("chunk"));
  assert!(PackedFormat::parse("unorm11-10-11").expect_err("an unknown format").contains("not supported"));
  // Two packed words fit where a vec3 would not.
  assert!(KeyColumn::parse(16.0, None, Some(2.0), Some(packed)).is_ok());
  assert!(KeyColumn::parse(16.0, None, Some(2.0), None).is_err());
  assert!(KeyColumn::parse(16.0, None, Some(3.0), Some(packed)).expect_err("past the end").contains("does not fit"));
}

// Records of 16 bytes: the packed position words at float 0 (x | y << 16,
// then z with junk in its high half), the rest junk; positions on the
// quantization grid of their chunk's box, so the float key and the
// packed key agree exactly.
fn packed_cloud(count: usize, chunk: usize) -> (Vec<u8>, Vec<[f32; 6]>, Vec<f32>) {
  let mut records = Vec::new();
  let mut bounds = Vec::new();
  let mut zs = Vec::new();
  let chunks = count.div_ceil(chunk);
  for c in 0..chunks {
    // Box c: each axis over 65535 steps of 2 (exact in f32 at these
    // magnitudes), boxes 1 << 17 apart.
    let min = [c as f32 * 131072.0, c as f32 * 131072.0, c as f32 * 131072.0];
    let max = [min[0] + 65535.0 * 2.0, min[1] + 65535.0 * 2.0, min[2] + 65535.0 * 2.0];
    bounds.push([min[0], min[1], min[2], max[0], max[1], max[2]]);
  }
  let mut state = 12345u32;
  let mut next = || {
    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    state >> 8
  };
  for i in 0..count {
    let c = i / chunk;
    let (qx, qy, qz) = (next() % 65536, next() % 65536, next() % 65536);
    records.extend_from_slice(&(qx | (qy << 16)).to_ne_bytes());
    records.extend_from_slice(&(qz | 0xEEEE_0000).to_ne_bytes());
    records.extend_from_slice(&[0xEEu8; 8]);
    zs.push(bounds[c][2] + qz as f32 * 2.0);
  }
  (records, bounds, zs)
}

// The packed sort over (id, word) pairs in a shuffled draw order gives the
// float sort's record order along the direction, and the mapped
// materialization writes record ids, not draw slots.
#[test]
fn packed_key_sorts_as_the_float_key_does() {
  let count = 700;
  let chunk = 256;
  let (records, bounds, zs) = packed_cloud(count, chunk);
  let packed = PackedKey { format: PackedFormat::Unorm16x3, chunk };
  let column = KeyColumn::parse(16.0, None, Some(0.0), Some(packed)).expect("parses");
  // The draw order: every record, reversed in blocks of 7.
  let mut order: Vec<u32> = (0..count as u32).collect();
  for block in order.chunks_mut(7) {
    block.reverse();
  }
  let table = PackedRecords { bounds: bounds.clone(), order: order.clone() };
  column.check_packed(records.len(), &table).expect("a valid hand-off");
  let mut pairs = Vec::new();
  column.extract_packed(&records, &order, &mut pairs);
  assert_eq!(pairs.len(), count * 12);

  let indexed = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), true, None, false, Some(16.0))
    .expect("parses")
    .mirror_order();
  let mut scratch = OrderScratch::default();
  order_permutation_packed(&indexed, packed, &pairs, &bounds, &mut scratch);
  let mut dst = vec![0u8; (count + 3) * 4];
  materialize_indices_mapped(scratch.perm(), &pairs, &mut dst);
  let ids: Vec<u32> = dst.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
  assert_eq!(&ids[count..], &[INDEX_NONE; 3]);
  // Descending z: each drawn record's z is no larger than the previous.
  let mut last = f32::INFINITY;
  let mut seen = vec![false; count];
  for &id in &ids[..count] {
    let z = zs[id as usize];
    assert!(z <= last, "record {id} at z {z} drawn after z {last}");
    last = z;
    assert!(!seen[id as usize], "record {id} drawn twice");
    seen[id as usize] = true;
  }
  // The same order the float key column gives (equal z keep draw-slot
  // order under the packed sort, slot order under the float one; the
  // generator makes z unique enough that both agree).
  let floats: Vec<u8> =
    (0..count).flat_map(|i| [0.0f32, 0.0, zs[i]].iter().flat_map(|f| f.to_ne_bytes()).collect::<Vec<u8>>()).collect();
  let mut whole = OrderScratch::default();
  order_permutation(&indexed, 12, &floats, &mut whole);
  let float_ids: Vec<u32> = whole.perm().to_vec();
  let float_z: Vec<f32> = float_ids.iter().map(|&i| zs[i as usize]).collect();
  let packed_z: Vec<f32> = ids[..count].iter().map(|&i| zs[i as usize]).collect();
  assert_eq!(packed_z, float_z, "the packed key sorts the depths the float key sorts");
}

// The packed hand-off's contract: a box per chunk, a draw order that is a
// permutation of the records.
#[test]
fn packed_hand_off_is_validated() {
  let (records, bounds, _) = packed_cloud(300, 256);
  let packed = PackedKey { format: PackedFormat::Unorm16x3, chunk: 256 };
  let column = KeyColumn::parse(16.0, None, Some(0.0), Some(packed)).expect("parses");
  let order: Vec<u32> = (0..300).collect();
  let short = PackedRecords { bounds: bounds[..1].to_vec(), order: order.clone() };
  assert!(column.check_packed(records.len(), &short).expect_err("one box for two chunks").contains("chunk table"));
  let missing = PackedRecords { bounds: bounds.clone(), order: order[..299].to_vec() };
  assert!(column.check_packed(records.len(), &missing).expect_err("299 ids").contains("lists 299"));
  let mut twice = order.clone();
  twice[5] = 7;
  let dup = PackedRecords { bounds: bounds.clone(), order: twice };
  assert!(column.check_packed(records.len(), &dup).expect_err("7 twice").contains("twice"));
  let mut past = order.clone();
  past[0] = 300;
  let out = PackedRecords { bounds: bounds.clone(), order: past };
  assert!(column.check_packed(records.len(), &out).expect_err("id 300").contains("past"));
  let float = KeyColumn::parse(16.0, None, Some(0.0), None).expect("parses");
  let table = PackedRecords { bounds, order };
  assert!(float.check_packed(records.len(), &table).expect_err("not packed").contains("not packed"));
}

// The sort thread takes a packed job: the ids come back mapped through
// the pairs, in depth order.
#[test]
fn sort_worker_sorts_a_packed_job() {
  let (records, bounds, zs) = packed_cloud(40, 256);
  let packed = PackedKey { format: PackedFormat::Unorm16x3, chunk: 256 };
  let column = KeyColumn::parse(16.0, None, Some(0.0), Some(packed)).expect("parses");
  let order: Vec<u32> = (0..40).rev().collect();
  let mut pairs = Vec::new();
  column.extract_packed(&records, &order, &mut pairs);
  let keys = Arc::new(pairs);
  let indexed = InstanceOrder::parse(None, Some(0.0), Some([0.0, 0.0, 1.0]), false, None, false, Some(16.0))
    .expect("parses")
    .mirror_order();
  let worker = OrderWorker::spawn().expect("a sort thread");
  let job = SortJob {
    entry: (1, 1),
    buffer: 9,
    generation: 1,
    order: indexed,
    stride: 12,
    packed: Some((packed, Arc::new(bounds))),
    keys: keys.clone(),
    len: keys.len(),
    perm: Vec::new(),
    block: vec![0u8; 44 * 4],
    done: None,
  };
  assert!(worker.submit(job));
  let mut results = Vec::new();
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
  while results.is_empty() && std::time::Instant::now() < deadline {
    results = worker.drain();
    std::thread::sleep(std::time::Duration::from_millis(1));
  }
  let result = results.pop().expect("the job came back");
  let ids: Vec<u32> = result.block.chunks_exact(4).map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]])).collect();
  assert_eq!(&ids[40..], &[INDEX_NONE; 4]);
  let mut last = f32::NEG_INFINITY;
  for &id in &ids[..40] {
    assert!(id < 40, "a record id, not a draw slot");
    assert!(zs[id as usize] >= last, "ascending z");
    last = zs[id as usize];
  }
  drop(worker);
}
