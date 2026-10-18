//! Instance draw order within one entry (gather-at-publish): the pure half.
//! A draw entry may declare an instance order - a key read from each instance
//! record - and the Context permutes the records into key order while they
//! move through the buffer-write lease, so the GPU consumes key order while
//! writers keep addressing stable slots. This module owns the key vocabulary,
//! the setup validation, and the sort/gather; it knows nothing about buffers,
//! entries, or threads (Context's registry in context/order.rs does), which
//! keeps it testable in isolation.
//!
//! The sort is an LSD radix over order-preserving u32 quantizations of the
//! f32 keys - counting passes, no comparisons - because ordered populations
//! are large (30k sprites, 100k+ splats) and republish per frame, so the
//! sort must be linear and allocation-free in steady state. It is stable:
//! equal keys keep slot order, so the untouched case reproduces today's
//! insertion-order draw exactly.
//!
//! Two materializations of one permutation: the gathered records (the
//! instance buffer holds the records themselves in key order) and, for a
//! population whose records live in a data texture the shader fetches by
//! id, the INDEX stream - the instance buffer holds the sorted record
//! indices as u32, K of them per instance record, and a re-sort uploads
//! 4 bytes per record instead of the record.

/// The id an index stream carries past the sorted population (every slot
/// of the last instance's group past the count, and the rest of the
/// buffer): the shader culls it before any fetch.
pub const INDEX_NONE: u32 = u32::MAX;

// Digit width of the LSD radix sort over the exact 32-bit key: 11 bits =
// 3 counting passes (11, 11, 10) with 2048-bucket histograms, one pass
// fewer than the 8-bit split at the price of a wider scatter; the sort
// order is the same at any width.
const RADIX_BITS: usize = 11;
const RADIX_BUCKETS: usize = 1 << RADIX_BITS;
const RADIX_PASSES: usize = (32 + RADIX_BITS - 1) / RADIX_BITS;

/// Where an entry's per-record sort key comes from. Offsets are BYTE offsets
/// into one instance record (the public API speaks float offsets; the flux
/// boundary converts through `InstanceOrder::parse`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderKey {
  /// The f32 at `offset` in each record - a y coordinate, an age, an
  /// explicit sort field the app writes.
  Field { offset: usize },
  /// dot(vec3 at `offset`, `direction`) - view depth of a record position
  /// along a caller-named direction. Core does arithmetic on caller-named
  /// data; no camera concept enters (the SharedSlot projection stance).
  Projected { offset: usize, direction: [f32; 3] },
}

impl OrderKey {
  /// The key's byte offset in its record.
  pub fn offset(&self) -> usize {
    match *self {
      OrderKey::Field { offset } | OrderKey::Projected { offset, .. } => offset,
    }
  }

  /// The bytes the key reads: one f32 for a field, a vec3 for a position.
  pub fn bytes(&self) -> usize {
    match self {
      OrderKey::Field { .. } => 4,
      OrderKey::Projected { .. } => 12,
    }
  }

  /// The same key at another byte offset.
  fn at(&self, offset: usize) -> OrderKey {
    match *self {
      OrderKey::Field { .. } => OrderKey::Field { offset },
      OrderKey::Projected { direction, .. } => OrderKey::Projected { offset, direction },
    }
  }
}

/// How a handed-off position is packed (`PackedKey`): unorms over the
/// record's chunk box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackedFormat {
  /// 16 bits per axis in two words: x in the low half of the first, y in
  /// its high half, z in the low half of the second (its high half is the
  /// record's to use) - the splat record.
  Unorm16x3,
}

impl PackedFormat {
  /// The API's name for a format.
  pub fn parse(name: &str) -> Result<PackedFormat, String> {
    match name {
      "unorm16x3" => Ok(PackedFormat::Unorm16x3),
      other => Err(format!("packed format '{other}' is not supported (unorm16x3)")),
    }
  }

  /// The bytes the packed position takes in the record.
  pub fn bytes(self) -> usize {
    match self {
      PackedFormat::Unorm16x3 => 8,
    }
  }

  // The three unorms as integers, x first, from the record's words at
  // `at` (native-endian).
  fn unpack(self, bytes: &[u8], at: usize) -> [u32; 3] {
    match self {
      PackedFormat::Unorm16x3 => {
        let xy = read_u32(bytes, at);
        let z = read_u32(bytes, at + 4);
        [xy & 0xffff, xy >> 16, z & 0xffff]
      }
    }
  }

  // The largest unorm per axis, as a float divisor.
  fn steps(self) -> [f32; 3] {
    match self {
      PackedFormat::Unorm16x3 => [65535.0; 3],
    }
  }
}

/// A position key handed off packed: the format's words at the column's
/// offset, decoded against the box of the record's chunk (record id /
/// `chunk`), whose table the hand-off brings (`PackedRecords`). The
/// mirror then holds (record id, packed words) records in the hand-off's
/// draw order, and the sort decodes a key from them and the chunk's box,
/// read in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedKey {
  pub format: PackedFormat,
  /// Records per chunk.
  pub chunk: usize,
}

/// The data beside a packed hand-off: the chunk table (each chunk's
/// position box as min xyz, max xyz) and the draw order (the record ids
/// in the order a count prefix dials: the population the sort orders).
pub struct PackedRecords {
  pub bounds: Vec<[f32; 6]>,
  pub order: Vec<u32>,
}

/// Bytes of one mirror record under a packed key: the record id and the
/// packed position words (`PackedFormat::bytes`, 8 for unorm16x3).
pub const PACKED_PAIR_BYTES: usize = 12;

/// The key column of a handed-off record: what the index materialization's
/// mirror keeps of each record. An indexed order never republishes its
/// records (a re-sort writes ids), so the mirror holds the bytes the sort
/// reads and nothing else - 12 per record for a position key, 4 for a
/// field key, 12 for a packed position (the id and its two words) - and a
/// 1M-record population costs the engine 12 MB, not the record block.
/// Declared with the hand-off (`transferRecords`' key argument: a byte
/// stride and a float offset at the API, plus the packing) and matched
/// against the entry's order when it attaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyColumn {
  /// The handed-off record's byte stride.
  pub stride: usize,
  /// The key's byte offset inside one record.
  pub offset: usize,
  /// The key's byte length in the record: 4 for a field, 12 for a float
  /// position, the format's for a packed one (8 for unorm16x3).
  pub len: usize,
  /// Some when the position is packed (`PackedKey`).
  pub packed: Option<PackedKey>,
}

impl KeyColumn {
  /// Parse the API-boundary shape: `stride` in bytes, exactly one of
  /// `field` or `position` as a float offset into the record, and for a
  /// position optionally its packing.
  pub fn parse(
    stride: f64,
    field: Option<f64>,
    position: Option<f64>,
    packed: Option<PackedKey>,
  ) -> Result<KeyColumn, String> {
    let stride = record_stride(stride, "record key stride")?;
    let key = match (field, position) {
      (Some(_), Some(_)) => return Err("a record key is either field or position, not both".to_string()),
      (None, None) => return Err("a record key needs field or position (a float offset into the record)".to_string()),
      (Some(f), None) => OrderKey::Field { offset: float_offset(f, "record key field")? },
      (None, Some(p)) => OrderKey::Projected { offset: float_offset(p, "record key position")?, direction: [0.0; 3] },
    };
    if let Some(packed) = packed {
      if matches!(key, OrderKey::Field { .. }) {
        return Err("a packed record key is a position; a field key is one f32".to_string());
      }
      if packed.chunk == 0 {
        return Err("a packed record key needs a positive chunk size".to_string());
      }
    }
    let len = match packed {
      Some(packed) => packed.format.bytes(),
      None => key.bytes(),
    };
    let column = KeyColumn { stride, offset: key.offset(), len, packed };
    if column.offset + column.len > column.stride {
      return Err(format!("record key at float {} does not fit the {}-byte record", column.offset / 4, column.stride));
    }
    Ok(column)
  }

  /// Bytes of one mirror record: the key's, or the id plus words of a
  /// packed position.
  pub fn mirror_len(&self) -> usize {
    if self.packed.is_some() {
      PACKED_PAIR_BYTES
    } else {
      self.len
    }
  }

  /// Whether an entry's declared column (`InstanceOrder::key_column`, a
  /// float key) reads what this hand-off holds: the same column exactly,
  /// or a position key over a packed hand-off's stride and offset.
  pub fn accepts(&self, declared: KeyColumn) -> bool {
    match self.packed {
      None => *self == declared,
      Some(_) => {
        declared.packed.is_none()
          && declared.len == 12
          && declared.stride == self.stride
          && declared.offset == self.offset
      }
    }
  }

  /// Whether `len` bytes are a whole number of this column's records.
  pub fn check_records(&self, len: usize) -> Result<(), String> {
    if len % self.stride != 0 {
      return Err(format!("{len} bytes is not a whole number of {}-byte records", self.stride));
    }
    Ok(())
  }

  /// Copy the key column out of `records` (a whole number of records, see
  /// `check_records`) into `out`, `len` bytes per record.
  pub fn extract(&self, records: &[u8], out: &mut Vec<u8>) {
    debug_assert!(records.len() % self.stride == 0, "hand-off not a whole number of records");
    out.clear();
    out.reserve(records.len() / self.stride * self.len);
    for record in records.chunks_exact(self.stride) {
      out.extend_from_slice(&record[self.offset..self.offset + self.len]);
    }
  }

  /// What a packed hand-off must bring for `records`: a draw order that
  /// lists every record once, and a box per chunk.
  pub fn check_packed(&self, records_len: usize, packed: &PackedRecords) -> Result<(), String> {
    let Some(key) = self.packed else {
      return Err("the record key is not packed; a chunk table and draw order go with a packed key".to_string());
    };
    let count = records_len / self.stride;
    let chunks = count.div_ceil(key.chunk);
    if packed.bounds.len() != chunks {
      return Err(format!(
        "the chunk table holds {} boxes; {count} records in chunks of {} need {chunks}",
        packed.bounds.len(),
        key.chunk
      ));
    }
    if packed.order.len() != count {
      return Err(format!("the draw order lists {} ids for {count} records", packed.order.len()));
    }
    let mut seen = vec![false; count];
    for &id in &packed.order {
      let Some(slot) = seen.get_mut(id as usize) else {
        return Err(format!("the draw order names record {id}, past the {count} handed off"));
      };
      if *slot {
        return Err(format!("the draw order names record {id} twice"));
      }
      *slot = true;
    }
    Ok(())
  }

  /// Build the packed mirror: (record id, packed words) in the draw
  /// order, native-endian, `PACKED_PAIR_BYTES` each (see `check_packed`
  /// for the contract on `order`).
  pub fn extract_packed(&self, records: &[u8], order: &[u32], out: &mut Vec<u8>) {
    debug_assert!(records.len() % self.stride == 0, "hand-off not a whole number of records");
    debug_assert!(self.len + 4 == PACKED_PAIR_BYTES, "a packed column's words fill the mirror record");
    out.clear();
    out.reserve(order.len() * PACKED_PAIR_BYTES);
    for &id in order {
      let at = id as usize * self.stride + self.offset;
      out.extend_from_slice(&id.to_ne_bytes());
      out.extend_from_slice(&records[at..at + self.len]);
    }
  }
}

/// One entry's declared instance order: the key source, which buffer's
/// records hold it, and the direction of the sort. Ascending draws
/// smallest key first; descending is the back-to-front alpha case when
/// larger keys are nearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceOrder {
  pub key: OrderKey,
  pub descending: bool,
  /// The pipeline buffer index whose records hold the key; None = the
  /// first instance-step buffer. On an entry with several instance-step
  /// buffers every one gathers under the key buffer's permutation,
  /// whoever writes it - a core-written pose buffer keyed by world y, or
  /// an app-written style buffer keyed by an explicit sort field.
  pub key_buffer: Option<usize>,
  /// The retained-copy strategy, for write-once populations (splat
  /// clouds): the registry keeps a CPU copy of each slot's published
  /// records, and a direction update re-sorts and republishes them
  /// core-side - no publish from the app. Position keys only (a field
  /// key re-orders when its records republish, so retaining buys nothing).
  pub retain: bool,
  /// The index materialization: Some(record stride in bytes) when the
  /// key buffer holds sorted record INDICES rather than records. The
  /// records are handed off (`instance_order_records`) with a `KeyColumn`
  /// declaring this stride and the key's offset, and the mirror keeps the
  /// key column only (`mirror_stride`, `mirror_order`); the buffer's
  /// instance record is `arrayStride / 4` u32 ids (K records per drawn
  /// instance), written whole on every re-sort with `INDEX_NONE` past the
  /// population. Retains by nature (the mirror is the only key copy).
  pub indices: Option<usize>,
}

impl InstanceOrder {
  /// Parse the API-boundary shape: `field` or `position` are float offsets
  /// into one instance record (exactly one of the two), `direction` is
  /// required with `position` and rejected with `field`, `buffer` names the
  /// pipeline buffer index holding the key (default: the first
  /// instance-step buffer), `retain` opts into the retained-copy strategy
  /// (position keys only), `indices` (a byte stride) selects the index
  /// materialization over handed-off records of that stride. Stored
  /// offsets are bytes.
  pub fn parse(
    field: Option<f64>,
    position: Option<f64>,
    direction: Option<[f32; 3]>,
    descending: bool,
    buffer: Option<f64>,
    retain: bool,
    indices: Option<f64>,
  ) -> Result<InstanceOrder, String> {
    let key_buffer = match buffer {
      None => None,
      Some(b) => {
        if !(b.is_finite() && b >= 0.0 && b.fract() == 0.0 && (b as usize) < super::vocab::MAX_BUFFERS) {
          return Err(format!("instanceOrder buffer must be an integer 0..{}, got {b}", super::vocab::MAX_BUFFERS - 1));
        }
        Some(b as usize)
      }
    };
    let key = match (field, position) {
      (Some(_), Some(_)) => return Err("instanceOrder takes either field or position, not both".to_string()),
      (None, None) => {
        return Err(
          "instanceOrder needs a key: field (float offset) or position (float offset) with direction".to_string(),
        )
      }
      (Some(f), None) => {
        if direction.is_some() {
          return Err("direction applies to a position key; a field key has none".to_string());
        }
        if retain {
          return Err("retain applies to a position key; a field key re-orders when its records republish".to_string());
        }
        OrderKey::Field { offset: float_offset(f, "instanceOrder field")? }
      }
      (None, Some(p)) => {
        let Some(direction) = direction else {
          return Err("a position key needs a direction (the view direction positions project onto)".to_string());
        };
        check_direction(direction)?;
        OrderKey::Projected { offset: float_offset(p, "instanceOrder position")?, direction }
      }
    };
    let indices = match indices {
      None => None,
      Some(s) => Some(record_stride(s, "instanceOrder indices.stride")?),
    };
    let order = InstanceOrder { key, descending, key_buffer, retain, indices };
    if let Some(stride) = indices {
      order.check_stride(stride)?;
    }
    Ok(order)
  }

  /// The byte stride of the records the sort reads off the mirror: the key
  /// column's under the index materialization (the mirror holds nothing
  /// else), the instance buffer's otherwise.
  pub fn mirror_stride(&self, instance_stride: usize) -> usize {
    if self.indices.is_some() {
      self.key.bytes()
    } else {
      instance_stride
    }
  }

  /// The order as the sort reads it off the mirror: under the index
  /// materialization the key rebased to offset 0 of the key column, else
  /// itself.
  pub fn mirror_order(&self) -> InstanceOrder {
    if self.indices.is_none() {
      return *self;
    }
    InstanceOrder { key: self.key.at(0), ..*self }
  }

  /// The key column an index-materialized order expects its records handed
  /// off as; None for the gathered forms.
  pub fn key_column(&self) -> Option<KeyColumn> {
    self.indices.map(|stride| KeyColumn { stride, offset: self.key.offset(), len: self.key.bytes(), packed: None })
  }

  /// The key bytes must sit inside one record of `stride` bytes: a field key
  /// reads one f32, a projected key reads a vec3.
  pub fn check_stride(&self, stride: usize) -> Result<(), String> {
    let name = match self.key {
      OrderKey::Field { .. } => "field",
      OrderKey::Projected { .. } => "position",
    };
    if self.key.offset() + self.key.bytes() > stride {
      return Err(format!(
        "instanceOrder {name} offset {} does not fit the {stride}-byte instance record",
        self.key.offset() / 4
      ));
    }
    Ok(())
  }

  /// Replace the projected key's direction (the per-camera-move update).
  /// On a gather entry it takes effect at the next publish (gather retains
  /// nothing to re-sort in place); on a retained entry the registry
  /// re-materializes from its copy right after (`rematerialize_retained_order`).
  pub fn set_direction(&mut self, direction: [f32; 3]) -> Result<(), String> {
    let OrderKey::Projected { offset, .. } = self.key else {
      return Err("the entry's instance order uses a field key; orderDirection applies to position keys".to_string());
    };
    check_direction(direction)?;
    self.key = OrderKey::Projected { offset, direction };
    Ok(())
  }
}

fn float_offset(value: f64, name: &str) -> Result<usize, String> {
  if !(value.is_finite() && value >= 0.0 && value.fract() == 0.0) {
    return Err(format!("{name} must be a non-negative integer float offset, got {value}"));
  }
  Ok(value as usize * 4)
}

// A record's byte stride at the API boundary: a positive multiple of 4
// (whole float records).
fn record_stride(value: f64, name: &str) -> Result<usize, String> {
  if !(value.is_finite() && value > 0.0 && value.fract() == 0.0 && (value as usize) % 4 == 0) {
    return Err(format!("{name} must be a positive multiple of 4 bytes (whole float records), got {value}"));
  }
  Ok(value as usize)
}

fn check_direction(direction: [f32; 3]) -> Result<(), String> {
  if direction.iter().any(|c| !c.is_finite()) {
    return Err("direction components must be finite".to_string());
  }
  if direction == [0.0, 0.0, 0.0] {
    return Err("direction must not be the zero vector".to_string());
  }
  Ok(())
}

/// The sort's working memory, held by the Context registry and reused across
/// publishes: after the first publish at a given population, a re-sort
/// allocates nothing.
#[derive(Default)]
pub struct OrderScratch {
  // Quantized key per slot (indexed by slot; never reordered).
  keys: Vec<u32>,
  // Per chunk of a packed key: dot(box min, direction) and the per-axis
  // unorm step along the direction.
  chunks: Vec<[f32; 4]>,
  // The permutation under construction: perm[i] = the slot drawn i-th.
  perm: Vec<u32>,
  // The ping-pong partner of `perm` for the radix passes.
  tmp: Vec<u32>,
  // All RADIX_PASSES histograms, built in one sweep over the keys.
  counts: Vec<u32>,
}

impl OrderScratch {
  /// The permutation `order_permutation` left behind (perm[i] = the slot
  /// drawn i-th) - what a multi-buffer entry retains and applies to sibling
  /// publishes.
  pub fn perm(&self) -> &[u32] {
    &self.perm
  }
}

// The f32 at byte offset `at` (native-endian, matching the Float32Array
// writer); goes through a copy because a lease block has no alignment
// guarantee.
fn read_f32(bytes: &[u8], at: usize) -> f32 {
  let mut b = [0u8; 4];
  b.copy_from_slice(&bytes[at..at + 4]);
  f32::from_ne_bytes(b)
}

// The u32 at byte offset `at`, native-endian (the mirror's own writes).
fn read_u32(bytes: &[u8], at: usize) -> u32 {
  let mut b = [0u8; 4];
  b.copy_from_slice(&bytes[at..at + 4]);
  u32::from_ne_bytes(b)
}

// Order-preserving f32 -> u32: flip all bits of negatives, only the sign bit
// of non-negatives, so unsigned order equals numeric order (-inf lowest,
// +inf highest, the standard radix float transform). NaN in the usual
// positive quiet pattern maps above +inf and sorts last ascending.
fn quantize(f: f32) -> u32 {
  let b = f.to_bits();
  if b >> 31 == 1 {
    !b
  } else {
    b ^ 0x8000_0000
  }
}

/// Gather `src`'s records into `dst` in key order. `src.len()` must be a
/// whole number of `stride`-byte records and `dst` at least as long - both
/// the caller's checks (the Context registry validates the publish length
/// against the entry's stride before calling). Stable: equal keys keep slot
/// order.
pub fn gather_ordered(order: &InstanceOrder, stride: usize, src: &[u8], dst: &mut [u8], scratch: &mut OrderScratch) {
  order_permutation(order, stride, src, scratch);
  gather_permuted(&scratch.perm, stride, src, dst);
}

/// Sort `src`'s records by key and leave the permutation in `scratch.perm`
/// (perm[i] = the slot drawn i-th) - the compute half of `gather_ordered`,
/// exposed so a multi-buffer entry can gather several buffers under ONE
/// permutation (computed from the key buffer, applied to its siblings).
/// Same contracts and stability as `gather_ordered`.
pub fn order_permutation(order: &InstanceOrder, stride: usize, src: &[u8], scratch: &mut OrderScratch) {
  debug_assert!(stride > 0 && src.len() % stride == 0, "publish length not a whole number of records");
  let count = src.len() / stride;
  let keys = &mut scratch.keys;
  keys.clear();
  keys.reserve(count);
  for slot in 0..count {
    let at = slot * stride;
    let f = match order.key {
      OrderKey::Field { offset } => read_f32(src, at + offset),
      OrderKey::Projected { offset, direction } => {
        read_f32(src, at + offset) * direction[0]
          + read_f32(src, at + offset + 4) * direction[1]
          + read_f32(src, at + offset + 8) * direction[2]
      }
    };
    let k = quantize(f);
    keys.push(if order.descending { !k } else { k });
  }
  radix_sort(scratch);
}

/// `order_permutation` over a packed mirror (`KeyColumn::extract_packed`):
/// `pairs` holds (record id, packed words) per draw slot, `bounds` the
/// chunk table, and the key is the decoded position's depth along the
/// order's direction - decoded as dot(box min, d) plus each unorm times
/// its axis step along d, three multiply-adds per record on the integers,
/// so the sort streams 12 bytes per record and the table stays in cache.
/// The permutation is over draw slots; `materialize_indices_mapped` turns
/// it into record ids.
pub fn order_permutation_packed(
  order: &InstanceOrder,
  key: PackedKey,
  pairs: &[u8],
  bounds: &[[f32; 6]],
  scratch: &mut OrderScratch,
) {
  debug_assert!(pairs.len() % PACKED_PAIR_BYTES == 0, "packed mirror not whole pairs");
  let OrderKey::Projected { direction, .. } = order.key else {
    debug_assert!(false, "a packed key is a position key");
    return;
  };
  let count = pairs.len() / PACKED_PAIR_BYTES;
  let steps = key.format.steps();
  let OrderScratch { keys, chunks, .. } = scratch;
  chunks.clear();
  chunks.reserve(bounds.len());
  for b in bounds {
    let base = b[0] * direction[0] + b[1] * direction[1] + b[2] * direction[2];
    chunks.push([
      base,
      (b[3] - b[0]) / steps[0] * direction[0],
      (b[4] - b[1]) / steps[1] * direction[1],
      (b[5] - b[2]) / steps[2] * direction[2],
    ]);
  }
  keys.clear();
  keys.reserve(count);
  for slot in 0..count {
    let at = slot * PACKED_PAIR_BYTES;
    let id = read_u32(pairs, at) as usize;
    let q = key.format.unpack(pairs, at + 4);
    // A record past the table (a hand-off the registry validated cannot
    // have one) keys as the box-less origin rather than reading out of
    // bounds.
    let c = chunks.get(id / key.chunk).copied().unwrap_or([0.0; 4]);
    let f = c[0] + q[0] as f32 * c[1] + q[1] as f32 * c[2] + q[2] as f32 * c[3];
    let k = quantize(f);
    keys.push(if order.descending { !k } else { k });
  }
  radix_sort(scratch);
}

// The LSD radix passes over `scratch.keys`, leaving the permutation in
// `scratch.perm` (perm[i] = the slot drawn i-th), stable.
fn radix_sort(scratch: &mut OrderScratch) {
  let OrderScratch { keys, perm, tmp, counts, .. } = scratch;
  let count = keys.len();
  perm.clear();
  perm.extend(0..count as u32);
  tmp.clear();
  tmp.resize(count, 0);
  counts.clear();
  counts.resize(RADIX_PASSES * RADIX_BUCKETS, 0);
  for &k in keys.iter() {
    for pass in 0..RADIX_PASSES {
      counts[pass * RADIX_BUCKETS + ((k as usize >> (pass * RADIX_BITS)) & (RADIX_BUCKETS - 1))] += 1;
    }
  }

  for pass in 0..RADIX_PASSES {
    let hist = &mut counts[pass * RADIX_BUCKETS..(pass + 1) * RADIX_BUCKETS];
    // Every key in one bucket = the pass would be the identity; skip it
    // (clustered keys make this the common case for the high digits).
    if hist.iter().any(|&c| c as usize == count) {
      continue;
    }
    let mut sum = 0u32;
    for c in hist.iter_mut() {
      let n = *c;
      *c = sum;
      sum += n;
    }
    let shift = pass * RADIX_BITS;
    for &slot in perm.iter() {
      let digit = (keys[slot as usize] as usize >> shift) & (RADIX_BUCKETS - 1);
      tmp[hist[digit] as usize] = slot;
      hist[digit] += 1;
    }
    std::mem::swap(perm, tmp);
  }
}

/// Write `perm` (perm[i] = the slot drawn i-th) into `dst` as native-endian
/// u32 ids - the index materialization - and fill the rest of `dst` with
/// `INDEX_NONE`, so every id slot of the buffer is either a sorted record
/// or the sentinel and no stale id from a larger population survives. A
/// trailing partial u32 (a `dst` length not a multiple of 4) is left alone.
pub fn materialize_indices(perm: &[u32], dst: &mut [u8]) {
  let slots = dst.len() / 4;
  for (i, chunk) in dst.chunks_exact_mut(4).enumerate() {
    let id = if i < perm.len() { perm[i] } else { INDEX_NONE };
    chunk.copy_from_slice(&id.to_ne_bytes());
  }
  debug_assert!(perm.len() <= slots, "permutation longer than the index buffer");
}

/// `materialize_indices` for a permutation over a packed mirror's draw
/// slots: the id written is the slot's record id, read from `pairs`.
pub fn materialize_indices_mapped(perm: &[u32], pairs: &[u8], dst: &mut [u8]) {
  let slots = dst.len() / 4;
  for (i, chunk) in dst.chunks_exact_mut(4).enumerate() {
    let id = if i < perm.len() { read_u32(pairs, perm[i] as usize * PACKED_PAIR_BYTES) } else { INDEX_NONE };
    chunk.copy_from_slice(&id.to_ne_bytes());
  }
  debug_assert!(perm.len() <= slots, "permutation longer than the index buffer");
}

/// Gather `src`'s records into `dst` following `perm` (perm[i] = the slot
/// drawn i-th). `dst` must hold at least `src.len()` bytes. The record
/// counts may disagree - a sibling buffer can publish more or fewer records
/// than the key buffer the permutation was computed from: perm entries past
/// `src`'s record count are skipped, records past the permutation's length
/// append in slot order. Every source record lands exactly once either way.
pub fn gather_permuted(perm: &[u32], stride: usize, src: &[u8], dst: &mut [u8]) {
  debug_assert!(stride > 0 && src.len() % stride == 0, "publish length not a whole number of records");
  debug_assert!(dst.len() >= src.len(), "gather destination smaller than the publish");
  let count = src.len() / stride;
  let mut out = 0usize;
  let mut place = |slot: usize| {
    dst[out * stride..(out + 1) * stride].copy_from_slice(&src[slot * stride..(slot + 1) * stride]);
    out += 1;
  };
  for &slot in perm {
    if (slot as usize) < count {
      place(slot as usize);
    }
  }
  for slot in perm.len()..count {
    place(slot);
  }
}
